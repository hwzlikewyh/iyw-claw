#!/usr/bin/env node

/**
 * Rebuild and sign a Windows NSIS bundle from a hosted-runner staging input.
 * CI authenticates the token inside each signing process using the configured PIN.
 */

import { createHash } from "node:crypto"
import {
  copyFileSync,
  cpSync,
  existsSync,
  openSync,
  readSync,
  closeSync,
  statSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs"
import { execFileSync, spawnSync } from "node:child_process"
import { dirname, join, resolve } from "node:path"
import { tmpdir } from "node:os"
import { fileURLToPath } from "node:url"
import process from "node:process"
import { windowsLayout, verifyWindowsStaging } from "./windows-staging.mjs"
import { discoverSigntool, signFiles } from "./sign-windows.mjs"
import { win7WebviewConfig } from "./build-desktop-windows.mjs"

const TOOL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..")
const ROOT = resolve(process.env.IYW_CLAW_BUILD_ROOT ?? TOOL_ROOT)
const TARGET = process.env.TAURI_TARGET_TRIPLE || "x86_64-pc-windows-msvc"
const LAYOUT = windowsLayout(TARGET)
const STAGING_ROOT = resolve(
  process.env.IYW_CLAW_STAGING_DIR ?? join(ROOT, ".staged-windows")
)
const CLI = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js")
const TARGET_RELEASE = join("src-tauri", "target", TARGET, "release")
const PREFLIGHT_TIMEOUT_MS = 12 * 60_000
const PROBE_PREPARE_TIMEOUT_MS = 90_000
const BUNDLE_TIMEOUT_MS = 30 * 60_000
const VERIFY_TIMEOUT_MS = 2 * 60_000

function fail(message) {
  throw new Error(message)
}

function verifyManifest() {
  const expectedVersion = JSON.parse(
    readFileSync(join(ROOT, "package.json"), "utf8")
  ).version
  const sourceCommit = execFileSync("git", ["rev-parse", "HEAD"], {
    cwd: ROOT,
    encoding: "utf8",
  }).trim()
  return verifyWindowsStaging(STAGING_ROOT, {
    version: expectedVersion,
    sourceCommit,
    target: TARGET,
  })
}

function restoreStaging(includesFrontend) {
  const directories = LAYOUT.directories.filter(
    (path) => path !== "out" || includesFrontend
  )
  for (const directory of directories) {
    const destination = join(ROOT, directory)
    rmSync(destination, { recursive: true, force: true })
    cpSync(join(STAGING_ROOT, directory), destination, { recursive: true })
  }
  const files = LAYOUT.files
  for (const file of files) {
    const destination = join(ROOT, file)
    mkdirSync(dirname(destination), { recursive: true })
    rmSync(destination, { force: true })
    cpSync(join(STAGING_ROOT, file), destination)
  }
  console.log("[staged-signing] restored verified staging inputs")
}

function preflightToken() {
  if (
    process.env.GITHUB_ACTIONS === "true" &&
    !(process.env.IYW_CLAW_SAFENET_PIN ?? "").trim()
  )
    fail(
      "CI signing requires IYW_CLAW_SAFENET_PIN; interactive signing is disabled"
    )
  const probe = join(tmpdir(), `iyw-signing-preflight-${process.pid}.exe`)
  // 在副本上移除原签名，再验证当前发布证书，原 Node 可执行文件保持不变。
  try {
    copyFileSync(process.execPath, probe)
    const unsigned = spawnSync(
      discoverSigntool(process.env),
      ["remove", "/s", probe],
      {
        cwd: ROOT,
        stdio: "inherit",
        windowsHide: true,
        timeout: PROBE_PREPARE_TIMEOUT_MS,
      }
    )
    if (unsigned.error) throw unsigned.error
    if (unsigned.status !== 0) fail("could not prepare unsigned signing probe")
    const result = spawnSync(
      process.execPath,
      [
        join(TOOL_ROOT, "src-tauri", "scripts", "sign-staged-windows.mjs"),
        probe,
      ],
      {
        cwd: ROOT,
        stdio: "inherit",
        windowsHide: true,
        timeout: PREFLIGHT_TIMEOUT_MS,
      }
    )
    if (result.error) throw result.error
    if (result.status !== 0)
      fail(`signing preflight failed with exit code ${result.status}`)
  } finally {
    rmSync(probe, { force: true })
  }
}

function prepareBundleConfig() {
  const config = join(tmpdir(), `iyw-staged-bundle-${process.pid}.json`)
  const bundle = { createUpdaterArtifacts: false }
  const webview = win7WebviewConfig(TARGET)
  if (webview) Object.assign(bundle, webview.bundle)
  writeFileSync(config, JSON.stringify({ bundle }) + "\n", "utf8")
  return config
}

function prepareSigningConfig() {
  const config = join(tmpdir(), `iyw-staged-signing-${process.pid}.json`)
  writeFileSync(
    config,
    `${JSON.stringify(
      {
        bundle: {
          windows: {
            // The bundler runs signCommand before wrapping the app into the NSIS
            // installer, so an installer-level signature is impossible here.
            // Sign the app explicitly (signApplication) and let NSIS wrap it.
            signCommand: null,
          },
        },
      },
      null,
      2
    )}\n`,
    "utf8"
  )
  return config
}

function signApplication() {
  const executable = join(ROOT, TARGET_RELEASE, "iyw-claw.exe")
  if (!existsSync(executable))
    fail(`staged application is missing: ${executable}`)
  signFiles([executable])
}

function bundleArgs(signingConfig, bundleConfig) {
  const args = [
    CLI,
    "bundle",
    "--target",
    TARGET,
    "--features",
    "tauri-runtime",
    "--bundles",
    "nsis",
    "--config",
    "src-tauri/tauri.ci.conf.json",
    "--config",
    signingConfig,
    "--config",
    bundleConfig,
  ]
  if (LAYOUT.overlay) args.splice(10, 0, "--config", LAYOUT.overlay)
  return args
}

function bundle(version) {
  const output = join(ROOT, TARGET_RELEASE, "bundle", "nsis")
  const suffix = `_${version}_${LAYOUT.arch}-setup.exe`
  mkdirSync(output, { recursive: true })
  // 保留其它版本及目录，旧安装器可能仍在使用其安装包。
  for (const entry of readdirSync(output, { withFileTypes: true })) {
    if (
      entry.isFile() &&
      (entry.name.endsWith(suffix) || entry.name.endsWith(`${suffix}.sig`))
    )
      rmSync(join(output, entry.name), { force: true })
  }
  signApplication()
  const signingConfig = prepareSigningConfig()
  const bundleConfig = prepareBundleConfig()
  try {
    const args = bundleArgs(signingConfig, bundleConfig)
    const result = spawnSync(process.execPath, args, {
      cwd: ROOT,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      windowsHide: false,
      timeout: BUNDLE_TIMEOUT_MS,
    })
    process.stdout.write(result.stdout ?? "")
    process.stderr.write(result.stderr ?? "")
    if (result.error) throw result.error
    const outputText = `${result.stdout ?? ""}${result.stderr ?? ""}`
    if (result.status !== 0)
      fail(`NSIS bundle failed with exit code ${result.status}`)
    const installers = readdirSync(output).filter((name) =>
      name.endsWith(suffix)
    )
    if (installers.length !== 1)
      fail(`expected one final installer, found ${installers.length}`)
    console.log(
      `[staged-signing] finalized ${version}: ${join(output, installers[0])}`
    )
    return join(output, installers[0])
  } finally {
    rmSync(bundleConfig, { force: true })
    rmSync(signingConfig, { force: true })
  }
}

function verify(installer) {
  const result = spawnSync(
    process.execPath,
    [join(TOOL_ROOT, "src-tauri", "scripts", "verify-signatures.mjs"), installer],
    {
      cwd: ROOT,
      stdio: "inherit",
      windowsHide: false,
      timeout: VERIFY_TIMEOUT_MS,
    }
  )
  if (result.error) throw result.error
  if (result.status !== 0)
    fail("final installer Authenticode verification failed")
}

/**
 * The bundler removes a pre-existing app signature when it wraps the app into
 * the NSIS installer, so "the bundler exited 0" proves nothing. Assert that the
 * installed PE carries a signature whose hash covers the file up to the
 * certificate table.
 */
function verifyPeSignature(installer) {
  const buffer = readFileSync(installer)
  if (buffer.readUInt16LE(0) !== 0x5a4d) fail(`not a PE file: ${installer}`)
  const peOffset = buffer.readInt32LE(0x3c)
  const optionalHeader = peOffset + 24
  const magic = buffer.readUInt16LE(optionalHeader)
  const dataDirectories = optionalHeader + (magic === 0x20b ? 112 : 96)
  const certificateOffset = buffer.readInt32LE(dataDirectories + 8 * 4 + 0)
  const certificateSize = buffer.readInt32LE(dataDirectories + 8 * 4 + 4)
  if (!certificateOffset || !certificateSize)
    fail("installer has no Authenticode certificate table")
  const certificate = buffer.subarray(certificateOffset, certificateOffset + certificateSize)
  const length = certificate.readUInt32LE(0)
  const revision = certificate.readUInt16LE(4)
  const type = certificate.readUInt16LE(6)
  if (revision !== 0x0200 || type !== 0x0002)
    fail("installer certificate table is not a PKCS#7 Authenticode signature")
  const digestAlgorithm = certificate.readUInt32LE(length + 8)
  const digestSize = certificate.readUInt32LE(length + 12)
  const digest = certificate.subarray(length + 16, length + 16 + digestSize)
  const algorithm = { 0x8004: "sha1", 0x800c: "sha256", 0x800e: "sha384", 0x800d: "sha512" }[digestAlgorithm]
  if (!algorithm) fail(`unsupported Authenticode digest 0x${digestAlgorithm.toString(16)}`)
  const hash = createHash(algorithm)
  hash.update(buffer.subarray(0, certificateOffset))
  const end = certificateOffset + certificateSize
  if (end < buffer.length) hash.update(buffer.subarray(end))
  if (!hash.digest().equals(digest))
    fail("installer Authenticode hash does not match its contents")
  return { algorithm, digestSize }
}

function signEnvironmentHelper() {
  const helper = join(
    ROOT,
    "src-tauri",
    "binaries",
    `iyw-environment-${TARGET}.exe`
  )
  const result = spawnSync(
    process.execPath,
    [
      join(TOOL_ROOT, "src-tauri", "scripts", "sign-staged-windows.mjs"),
      helper,
    ],
    {
      cwd: ROOT,
      stdio: "inherit",
      windowsHide: false,
      timeout: PREFLIGHT_TIMEOUT_MS,
    }
  )
  if (result.error) throw result.error
  if (result.status !== 0)
    fail(`environment helper signing failed with exit code ${result.status}`)
}

function main() {
  if (process.platform !== "win32")
    fail("staged Windows signing requires Windows")
  const { version, includesFrontend } = verifyManifest()
  restoreStaging(includesFrontend)
  preflightToken()
  signEnvironmentHelper()
  // hosted 编译阶段没有发布证书，分享 EXE 的嵌入身份必须在签名阶段生成。
  execFileSync(
    process.execPath,
    [
      join(ROOT, "src-tauri/scripts/prepare-native-share.mjs"),
      "--target",
      TARGET,
    ],
    {
      cwd: ROOT,
      stdio: "inherit",
      windowsHide: true,
      env: { ...process.env, IYW_SHARE_UNSIGNED: "0" },
    }
  )
  const installer = bundle(version)
  const signature = verifyPeSignature(installer)
  console.log(
    `[staged-signing] installer Authenticode covered (${signature.algorithm})`
  )
  verify(installer)
}

try {
  main()
} catch (error) {
  console.error(`[staged-signing][ERROR] ${error.message}`)
  process.exit(1)
}
