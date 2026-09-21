#!/usr/bin/env node

/**
 * Rebuild and sign a Windows NSIS bundle from a hosted-runner staging input.
 * The caller must provide an already authenticated SafeNet session.
 */

import {
  cpSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
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
const PREFLIGHT_TIMEOUT_MS = 12 * 60_000
const UNLOCK_TIMEOUT_MS = 90_000
const BUNDLE_TIMEOUT_MS = 30 * 60_000
const VERIFY_TIMEOUT_MS = 2 * 60_000
const PROBE_ATTEMPTS = 3
const PROBE_RETRY_DELAY_MS = 15_000

function sleepSync(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms)
}


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
  // Keep the probe tiny. Copying node.exe made an ~89 MB probe; a large image
  // is needless work for a call whose only job is to prove the token answers.
  // Use a small real PE from the system instead.
  //
  // The token itself intermittently goes silent: signtool sits at ~1s CPU and
  // never returns, so the call hits its timeout. Observed on both an ~89 MB
  // node.exe copy and a 7 KB system DLL, so size is not the trigger -- the
  // session is. Retrying clears it, so probe a few times before giving up and
  // report the real cause so the next failure is not misread as a bad
  // certificate.
  const probeImage = readSmallestProbeImage()
  try {
    unlockToken()
    let lastReason = "unknown"
    for (let attempt = 1; attempt <= PROBE_ATTEMPTS; attempt += 1) {
      // Write then copy, and sync before signing: signing in the same instant
      // as the write let the verify step read stale attributes on the runner.
      const staged = `${probe}.${attempt}`
      writeFileSync(staged, probeImage)
      cpSync(staged, probe)
      rmSync(staged, { force: true })
      const result = spawnSync(
        process.execPath,
        [
          join(TOOL_ROOT, "src-tauri", "scripts", "sign-staged-windows.mjs"),
          probe,
        ],
        {
          cwd: ROOT,
          stdio: "inherit",
          windowsHide: false,
          timeout: PREFLIGHT_TIMEOUT_MS,
        }
      )
      if (!result.error && result.status === 0) return
      lastReason = result.error?.code ?? `exit ${result.status}`
      if (attempt === PROBE_ATTEMPTS) break
      console.warn(
        `[staged-signing][WARN] token probe attempt ${attempt}/${PROBE_ATTEMPTS} failed (${lastReason}); retrying`
      )
      sleepSync(PROBE_RETRY_DELAY_MS)
    }
    fail(
      `SafeNet preflight failed after ${PROBE_ATTEMPTS} attempts (${lastReason}); ` +
        "the hardware token did not respond to signtool"
    )
  } finally {
    rmSync(probe, { force: true })
  }
}

// Pick the smallest valid PE image already present on the machine. The token
// probe only needs a well-formed signing target, so a few-hundred-kilobyte
// system DLL is ideal and removes any dependency on shipping our own blob.
function readSmallestProbeImage() {
  const roots = [
    process.env.SystemRoot ? join(process.env.SystemRoot, "System32") : null,
    process.env.SystemRoot
      ? join(process.env.SystemRoot, "SysWOW64")
      : null,
  ].filter(Boolean)
  let best = null
  let bestSize = Number.POSITIVE_INFINITY
  for (const root of roots) {
    let entries
    try {
      entries = readdirSync(root, { withFileTypes: true })
    } catch {
      continue
    }
    for (const entry of entries) {
      if (!entry.isFile()) continue
      if (!/\.(dll|exe)$/i.test(entry.name)) continue
      const candidate = join(root, entry.name)
      let size
      try {
        size = statSync(candidate).size
      } catch {
        continue
      }
      // Skip anything the token would choke on, and anything too tiny to be a
      // real image.
      if (size < 1024 || size > 256 * 1024) continue
      if (size >= bestSize) continue
      bestSize = size
      best = candidate
    }
  }
  if (!best) fail("no small PE image available for the signing token probe")
  const image = readFileSync(best)
  // Guard against picking a non-PE file (MZ magic + PE header signature).
  if (image.length < 0x40 || image[0] !== 0x4d || image[1] !== 0x5a)
    fail(`probe candidate is not a PE image: ${best}`)
  const peOffset = image.readUInt32LE(0x3c)
  if (peOffset + 4 > image.length || image.readUInt32LE(peOffset) !== 0x00004550)
    fail(`probe candidate has no PE header: ${best}`)
  console.log(
    `[staged-signing] token probe image=${best} size=${image.length}`
  )
  return image
}

// KSP 与 PKCS#11 会话不等价；解锁后仍由真实签名探针判定是否可用。
function unlockToken() {
  const pin = (process.env.IYW_CLAW_SAFENET_PIN ?? "").trim()
  if (pin === "") {
    if (process.env.GITHUB_ACTIONS === "true")
      fail(
        "CI signing requires IYW_CLAW_SAFENET_PIN; interactive token login is unavailable"
      )
    console.log(
      "[staged-signing] IYW_CLAW_SAFENET_PIN is not set; leaving the token login to signtool"
    )
    return
  }
  const kspScript = join(
    TOOL_ROOT,
    "src-tauri",
    "scripts",
    "unlock-signing-ksp.mjs"
  )
  const kspResult = spawnSync(process.execPath, [kspScript], {
    cwd: ROOT,
    stdio: "inherit",
    windowsHide: true,
    timeout: UNLOCK_TIMEOUT_MS,
  })
  if (kspResult.error) throw kspResult.error
  if (kspResult.status !== 0) {
    fail(
      "could not unlock the signing token through the CNG KSP with the configured PIN (IYW_CLAW_SAFENET_PIN)"
    )
  }
  // Do NOT also open a PKCS#11 session here. signtool signs through the CNG
  // KSP above, and the extra middleware login destabilised the token: jobs
  // logged "unexpected PKCS#11 host failure" and then signtool returned
  // success while producing no signature, so the probe failed on a token that
  // was working a moment earlier. The KSP unlock is sufficient on its own.
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
      timeout: BUNDLE_TIMEOUT_MS,
    })
    process.stdout.write(result.stdout ?? "")
    process.stderr.write(result.stderr ?? "")
    if (result.error) throw result.error
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
