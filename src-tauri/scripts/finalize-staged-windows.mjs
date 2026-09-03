#!/usr/bin/env node

/**
 * Rebuild and sign a Windows NSIS bundle from a hosted-runner staging input.
 * The caller must provide an already authenticated SafeNet session.
 */

import { createHash } from "node:crypto"
import {
  copyFileSync,
  cpSync,
  existsSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { execFileSync, spawnSync } from "node:child_process"
import { dirname, isAbsolute, join, relative, resolve } from "node:path"
import { tmpdir } from "node:os"
import { fileURLToPath } from "node:url"
import process from "node:process"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..")
const TARGET = "x86_64-pc-windows-msvc"
const STAGING_ROOT = resolve(
  process.env.IYW_CLAW_STAGING_DIR ?? join(ROOT, ".staged-windows")
)
const MANIFEST_PATH = join(STAGING_ROOT, "staging-manifest.json")
const CLI = join(ROOT, "node_modules", "@tauri-apps", "cli", "tauri.js")
const TARGET_RELEASE = join("src-tauri", "target", TARGET, "release")
const ALLOWED_PREFIXES = [
  "out/",
  "src-tauri/binaries/",
  "src-tauri/resources/runtime-seed/",
]
const ALLOWED_FILES = new Set([
  "src-tauri/tauri.runtime-seed.conf.json",
  `${TARGET_RELEASE.replaceAll("\\", "/")}/iyw-claw.exe`,
])

function fail(message) {
  throw new Error(message)
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function allowedStagingPath(path) {
  return (
    ALLOWED_FILES.has(path) ||
    ALLOWED_PREFIXES.some((prefix) => path.startsWith(prefix))
  )
}

function verifyManifest() {
  if (!existsSync(MANIFEST_PATH)) fail(`missing ${MANIFEST_PATH}`)
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, "utf8"))
  const expectedVersion = JSON.parse(
    readFileSync(join(ROOT, "package.json"), "utf8")
  ).version
  const sourceCommit = execFileSync("git", ["rev-parse", "HEAD"], {
    cwd: ROOT,
    encoding: "utf8",
  }).trim()
  if (manifest.schemaVersion !== 1) fail("unsupported staging manifest schema")
  if (manifest.version !== expectedVersion) {
    fail(
      `staging version ${manifest.version} does not match ${expectedVersion}`
    )
  }
  if (manifest.sourceCommit !== sourceCommit) {
    fail(
      `staging commit ${manifest.sourceCommit} does not match ${sourceCommit}`
    )
  }
  if (manifest.target !== TARGET) fail(`staging target must be ${TARGET}`)
  if (!Array.isArray(manifest.files) || manifest.files.length === 0) {
    fail("staging manifest has no files")
  }
  const stagedPaths = new Set(manifest.files.map((entry) => entry?.path))
  for (const required of ALLOWED_FILES) {
    if (!stagedPaths.has(required))
      fail(`staging manifest is missing ${required}`)
  }
  for (const entry of manifest.files) {
    if (!entry || typeof entry.path !== "string" || isAbsolute(entry.path)) {
      fail("staging manifest contains an unsafe path")
    }
    if (!allowedStagingPath(entry.path))
      fail(`unexpected staged file: ${entry.path}`)
    const path = resolve(STAGING_ROOT, entry.path)
    if (relative(STAGING_ROOT, path).startsWith("..")) {
      fail(`file escapes staging root: ${entry.path}`)
    }
    if (!existsSync(path) || !statSync(path).isFile()) {
      fail(`missing staged file: ${entry.path}`)
    }
    const actualSize = statSync(path).size
    const actualHash = sha256(path)
    if (actualSize !== entry.size || actualHash !== entry.sha256) {
      fail(`staged file changed: ${entry.path}`)
    }
  }
  console.log(`[staged-signing] verified ${manifest.files.length} staged files`)
  return manifest.version
}

function restoreStaging() {
  const directories = [
    "out",
    join("src-tauri", "binaries"),
    join("src-tauri", "resources", "runtime-seed"),
  ]
  for (const directory of directories) {
    const destination = join(ROOT, directory)
    rmSync(destination, { recursive: true, force: true })
    cpSync(join(STAGING_ROOT, directory), destination, { recursive: true })
  }
  const files = [
    join("src-tauri", "tauri.runtime-seed.conf.json"),
    join(TARGET_RELEASE, "iyw-claw.exe"),
  ]
  for (const file of files) {
    const destination = join(ROOT, file)
    rmSync(destination, { force: true })
    cpSync(join(STAGING_ROOT, file), destination)
  }
  console.log("[staged-signing] restored verified staging inputs")
}

function preflightToken() {
  const probe = join(tmpdir(), `iyw-signing-preflight-${process.pid}.exe`)
  copyFileSync(process.execPath, probe)
  try {
    const result = spawnSync(
      process.execPath,
      [join(ROOT, "src-tauri", "scripts", "sign-windows.mjs"), probe],
      { cwd: ROOT, stdio: "inherit", windowsHide: false }
    )
    if (result.error) throw result.error
    if (result.status !== 0)
      fail(`SafeNet preflight failed with exit code ${result.status}`)
  } finally {
    rmSync(probe, { force: true })
  }
}

function prepareBundleConfig() {
  const config = join(tmpdir(), `iyw-staged-bundle-${process.pid}.json`)
  writeFileSync(config, '{"bundle":{"createUpdaterArtifacts":false}}\n', "utf8")
  return config
}

function bundle(version) {
  const output = join(ROOT, TARGET_RELEASE, "bundle", "nsis")
  rmSync(output, { recursive: true, force: true })
  const signingConfig = execFileSync(
    process.execPath,
    [join(ROOT, "src-tauri", "scripts", "prepare-signing-config.mjs")],
    { cwd: ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }
  ).trim()
  if (!signingConfig) fail("signing overlay was not generated")
  const bundleConfig = prepareBundleConfig()
  try {
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
      "src-tauri/tauri.runtime-seed.conf.json",
      "--config",
      signingConfig,
      "--config",
      bundleConfig,
    ]
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
    if (outputText.includes("[sign-windows][ERROR]")) {
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
  const version = verifyManifest()
  restoreStaging()
  preflightToken()
  verify(bundle(version))
}

try {
  main()
} catch (error) {
  console.error(`[staged-signing][ERROR] ${error.message}`)
  process.exit(1)
}
