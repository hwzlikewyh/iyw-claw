#!/usr/bin/env node
// Verifies bundled sidecars from staging through disposable Windows installs.
import { execFileSync } from "node:child_process"
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
} from "node:fs"
import { createHash } from "node:crypto"
import { dirname, join, resolve } from "node:path"
import { tmpdir } from "node:os"
import { fileURLToPath } from "node:url"
import process from "node:process"
import { verifyInstalledRuntimeSeed } from "./runtime-seed-bundle-verification.mjs"
import { verifyArtifacts } from "./verify-signatures.mjs"

import {
  addAgentBrowserHash,
  verifyAgentBrowserConfig,
  verifyInstalledAgentBrowser,
  verifyStagedAgentBrowser,
} from "./verify-agent-browser-bundle.mjs"
import {
  assertCleanInstallState,
  assertDisposableRunner,
  cleanupInstall,
  createSmokeTestId,
  installerTestArgs,
} from "./nsis-smoke-windows.mjs"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const SRC_TAURI = resolve(SCRIPT_DIR, "..")
const REPO_ROOT = resolve(SRC_TAURI, "..")
function log(message) {
  console.log(`[verify-sidecar-bundle] ${message}`)
}

function die(message) {
  throw new Error(message)
}

function parseArgs(argv) {
  const args = {
    installedApp: null,
    installer: null,
    target: null,
    version: null,
  }
  const keys = {
    "--installed-app": "installedApp",
    "--installer": "installer",
    "--target": "target",
    "--version": "version",
  }
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    const value = argv[index + 1]
    if (Object.hasOwn(keys, arg)) {
      if (!value || value.startsWith("--")) die(`missing value for ${arg}`)
      args[keys[arg]] = value
      index += 1
    } else {
      die(`unknown argument: ${arg}`)
    }
  }
  return args
}

function readVersion(args) {
  if (args.version) return args.version
  return JSON.parse(readFileSync(join(REPO_ROOT, "package.json"), "utf8"))
    .version
}

function resolveHostTarget() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const line = output.split(/\r?\n/).find((item) => item.startsWith("host:"))
  if (!line) die("rustc -vV did not return a host target triple")
  return line.slice("host:".length).trim()
}

function resolveTarget(args) {
  return args.target || process.env.TAURI_TARGET_TRIPLE || resolveHostTarget()
}

function requireNonEmptyFile(path, label) {
  if (!existsSync(path)) die(`${label} is missing: ${path}`)
  const stats = lstatSync(path)
  if (!stats.isFile() || stats.size === 0) {
    die(`${label} must be a non-empty regular file: ${path}`)
  }
  return stats
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function logFile(label, path, version) {
  const stats = requireNonEmptyFile(path, label)
  log(
    `${label}: path=${path} version=${version} size=${stats.size} sha256=${sha256(path)}`
  )
  return stats
}

function verifyConfiguredExternalBins(target) {
  verifyAgentBrowserConfig(SRC_TAURI, target, die)
}

function rejectLegacyMcpSidecars(directory) {
  if (!existsSync(directory)) return
  const legacy = readdirSync(directory).filter((name) =>
    /^iyw-claw-mcp(?:-|\.|$)/i.test(name)
  )
  if (legacy.length > 0) {
    die(
      `legacy MCP sidecars must be removed before bundling: ${legacy.join(", ")}`
    )
  }
}

function verifyStagedSidecars(target, version) {
  verifyConfiguredExternalBins(target)
  rejectLegacyMcpSidecars(join(SRC_TAURI, "binaries"))
  verifyStagedAgentBrowser(SRC_TAURI, target, { die, logFile, sha256 })
}

function resolveInstallerPath(args, target, version) {
  if (args.installer) return resolve(args.installer)
  const architecture = target.startsWith("i686") ? "x86" : "x64"
  const directories = [
    join(SRC_TAURI, "target", target, "release", "bundle", "nsis"),
    join(SRC_TAURI, "target", "release", "bundle", "nsis"),
  ]
  for (const directory of directories) {
    if (!existsSync(directory)) continue
    const match = readdirSync(directory)
      .filter(
        (file) =>
          file.endsWith("-setup.exe") &&
          file.includes(`_${version}_${architecture}-setup.exe`)
      )
      .sort((left, right) => left.length - right.length)[0]
    if (match) return join(directory, match)
  }
  die(`NSIS installer for v${version} (${target}) was not produced`)
}

function resolveInstalledApp(directory) {
  const appDirectory = join(directory, "app")
  if (existsSync(appDirectory)) return appDirectory
  if (existsSync(join(directory, "iyw-claw.exe"))) return directory
  die(`installed application directory is missing: ${directory}`)
}

function verifyInstalledSidecars(appDirectory, target, expectedHashes = null) {
  if ((process.env.IYW_CLAW_SIGN_MODE ?? "none") !== "none") {
    verifyArtifacts([join(appDirectory, "iyw-claw.exe")])
  }
  verifyInstalledAgentBrowser(appDirectory, target, expectedHashes, {
    die,
    logFile,
    sha256,
  })
  verifyInstalledRuntimeSeed(appDirectory, target, die)
}

function stagedHashes(target) {
  const hashes = new Map()
  addAgentBrowserHash(hashes, SRC_TAURI, target, sha256)
  return hashes
}

function logInstallRoot(root) {
  try {
    const entries = readdirSync(root, { recursive: true })
    if (entries.length === 0) {
      log("NSIS temporary root is empty after installer failure")
      return
    }
    log("NSIS temporary root contents after installer failure:")
    for (const entry of entries.slice(0, 200)) {
      const path = join(root, entry)
      let size = "unknown"
      try {
        size = String(lstatSync(path).size)
      } catch {
        // The installer may remove a file while the diagnostic snapshot runs.
      }
      log(`  ${entry} (${size} bytes)`)
    }
    if (entries.length > 200) log(`  ... ${entries.length - 200} more entries`)
  } catch (error) {
    log(`could not inspect NSIS temporary root: ${error.message}`)
  }
}

function finishNsisCleanup(cleanup, failure) {
  let warnings = []
  try {
    warnings = cleanupInstall(cleanup)
  } catch (error) {
    warnings.push(error.message)
  }
  if (warnings.length > 0) {
    const message = `NSIS smoke cleanup failed: ${warnings.join("; ")}`
    if (failure)
      throw new Error(`${failure.message}; ${message}`, { cause: failure })
    die(message)
  }
  log(`removed temporary NSIS installation root: ${cleanup.smokeRoot}`)
  if (failure) throw failure
}

function verifyNsisInstaller(installer, target, version) {
  if (process.platform !== "win32") die("NSIS verification requires Windows")
  assertDisposableRunner()
  assertCleanInstallState()
  logFile("NSIS installer", installer, version)
  const testId = createSmokeTestId()
  const installRoot = join(tmpdir(), `iyw-claw-nsis-smoke-${testId}`)
  mkdirSync(installRoot)
  const cleanup = { smokeRoot: installRoot, installRoot, testId }
  let failure
  try {
    log(`installing NSIS bundle into temporary root: ${installRoot}`)
    try {
      execFileSync(
        installer,
        ["/S", ...installerTestArgs(testId), `/D=${installRoot}`],
        {
          encoding: "utf8",
          stdio: ["ignore", "pipe", "pipe"],
        }
      )
    } catch (error) {
      log(`NSIS installer exit status: ${error.status ?? "unknown"}`)
      if (error.stdout) log(`NSIS stdout: ${String(error.stdout).trim()}`)
      if (error.stderr) log(`NSIS stderr: ${String(error.stderr).trim()}`)
      logInstallRoot(installRoot)
      throw error
    }
    verifyInstalledSidecars(
      resolveInstalledApp(installRoot),
      target,
      stagedHashes(target)
    )
  } catch (error) {
    failure = error
  }
  finishNsisCleanup(cleanup, failure)
}

function main() {
  const args = parseArgs(process.argv.slice(2))
  const target = resolveTarget(args)
  const version = readVersion(args)
  const verifyNsis = Boolean(
    args.installer || process.env.IYW_CLAW_VERIFY_NSIS === "1"
  )
  if (verifyNsis) assertDisposableRunner()
  if (args.installedApp) {
    if (verifyNsis) {
      die("--installed-app cannot be combined with NSIS verification")
    }
    verifyInstalledSidecars(
      resolveInstalledApp(resolve(args.installedApp)),
      target
    )
    return
  }
  verifyStagedSidecars(target, version)
  if (verifyNsis) {
    verifyNsisInstaller(
      resolveInstallerPath(args, target, version),
      target,
      version
    )
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    main()
  } catch (error) {
    console.error(`[verify-sidecar-bundle][ERROR] ${error.message}`)
    process.exit(1)
  }
}
