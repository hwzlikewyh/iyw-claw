#!/usr/bin/env node

/**
 * Rebuild and sign a Windows NSIS bundle from a hosted-runner staging input.
 * The caller must provide an already authenticated SafeNet session.
 */

import {
  copyFileSync,
  cpSync,
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

const TOOL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..")
const ROOT = resolve(process.env.IYW_CLAW_BUILD_ROOT ?? TOOL_ROOT)
const TARGET = process.env.TAURI_TARGET_TRIPLE || "x86_64-pc-windows-msvc"
const LAYOUT = windowsLayout(TARGET)
const STAGING_ROOT = resolve(
  process.env.IYW_CLAW_STAGING_DIR ?? join(ROOT, ".staged-windows")
)
const CLI = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js")
const TARGET_RELEASE = join("src-tauri", "target", TARGET, "release")

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
  const probe = join(tmpdir(), `iyw-signing-preflight-${process.pid}.exe`)
  copyFileSync(process.execPath, probe)
  try {
    unlockToken()
    const result = spawnSync(
      process.execPath,
      [
        join(TOOL_ROOT, "src-tauri", "scripts", "sign-staged-windows.mjs"),
        probe,
      ],
      { cwd: ROOT, stdio: "inherit", windowsHide: false }
    )
    if (result.error) throw result.error
    if (result.status !== 0)
      fail(`SafeNet preflight failed with exit code ${result.status}`)
  } finally {
    rmSync(probe, { force: true })
  }
}

/**
 * Logs the SafeNet/eToken signing token in via PKCS#11 so signtool never blocks
 * on the interactive "Token Logon" dialog.
 *
 * The token caches the user PIN per Windows logon session, so one successful
 * login covers every signature in this job. When the runner has no PIN
 * configured we keep the old behaviour: signtool prompts, and the bounded
 * timeout turns a missing login into a clear failure instead of a hung job.
 */
function unlockToken() {
  const pin = (process.env.IYW_CLAW_SAFENET_PIN ?? "").trim()
  if (pin === "") {
    console.log(
      "[staged-signing] IYW_CLAW_SAFENET_PIN is not set; leaving the token login to signtool"
    )
    return
  }
  const script = join(TOOL_ROOT, "src-tauri", "scripts", "unlock-signing-token.mjs")
  const result = spawnSync(process.execPath, [script], {
    cwd: ROOT,
    stdio: "inherit",
    windowsHide: true,
  })
  if (result.error) throw result.error
  if (result.status !== 0) {
    fail(
      "could not unlock the signing token with the configured PIN (IYW_CLAW_SAFENET_PIN)"
    )
  }
}

function prepareBundleConfig() {
  const config = join(tmpdir(), `iyw-staged-bundle-${process.pid}.json`)
  writeFileSync(config, '{"bundle":{"createUpdaterArtifacts":false}}\n', "utf8")
  return config
}

function prepareSigningConfig() {
  const config = join(tmpdir(), `iyw-staged-signing-${process.pid}.json`)
  const signer = join(
    TOOL_ROOT,
    "src-tauri",
    "scripts",
    "sign-staged-windows.mjs"
  )
  writeFileSync(
    config,
    `${JSON.stringify(
      {
        bundle: {
          windows: {
            signCommand: { cmd: process.execPath, args: [signer, "%1"] },
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

function bundleArgs(signingConfig, bundleConfig) {
  return [
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
    LAYOUT.overlay,
    "--config",
    signingConfig,
    "--config",
    bundleConfig,
  ]
}

function bundle(version) {
  const output = join(ROOT, TARGET_RELEASE, "bundle", "nsis")
  rmSync(output, { recursive: true, force: true })
  const signingConfig = prepareSigningConfig()
  const bundleConfig = prepareBundleConfig()
  try {
    const args = bundleArgs(signingConfig, bundleConfig)
    const result = spawnSync(process.execPath, args, {
      cwd: ROOT,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      windowsHide: false,
    })
    if (result.error) throw result.error
    process.stdout.write(result.stdout ?? "")
    process.stderr.write(result.stderr ?? "")
    const outputText = `${result.stdout ?? ""}${result.stderr ?? ""}`
    if (outputText.includes("[sign-staged-windows][ERROR]")) {
      fail("a signing command failed even though the bundler continued")
    }
    if (result.status !== 0)
      fail(`NSIS bundle failed with exit code ${result.status}`)
    const installers = readdirSync(output).filter((name) =>
      name.endsWith("-setup.exe")
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
    [join(ROOT, "src-tauri", "scripts", "verify-signatures.mjs"), installer],
    { cwd: ROOT, stdio: "inherit", windowsHide: false }
  )
  if (result.error) throw result.error
  if (result.status !== 0)
    fail("final installer Authenticode verification failed")
}

function main() {
  if (process.platform !== "win32")
    fail("staged Windows signing requires Windows")
  const { version, includesFrontend } = verifyManifest()
  restoreStaging(includesFrontend)
  preflightToken()
  verify(bundle(version))
}

try {
  main()
} catch (error) {
  console.error(`[staged-signing][ERROR] ${error.message}`)
  process.exit(1)
}
