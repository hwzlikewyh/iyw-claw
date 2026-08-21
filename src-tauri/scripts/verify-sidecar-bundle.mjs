#!/usr/bin/env node
// Verifies bundled sidecars from staging through disposable Windows installs.
import { execFileSync } from "node:child_process"
import {
  existsSync,
  lstatSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
} from "node:fs"
import { createHash } from "node:crypto"
import { dirname, join, resolve } from "node:path"
import { tmpdir } from "node:os"
import { fileURLToPath } from "node:url"
import process from "node:process"

import {
  addAgentBrowserHash,
  verifyAgentBrowserConfig,
  verifyInstalledAgentBrowser,
  verifyStagedAgentBrowser,
} from "./verify-agent-browser-bundle.mjs"

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url))
const SRC_TAURI = resolve(SCRIPT_DIR, "..")
const REPO_ROOT = resolve(SRC_TAURI, "..")
const BIN_NAME = "iyw-claw-mcp"
const NSIS_INSTALL_PREFIX = "iyw-claw-sidecar-"
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

function fileExtension(target) {
  return target.includes("windows") ? ".exe" : ""
}

function expectedNames(version) {
  return [BIN_NAME, `${BIN_NAME}-${version}`]
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

function expectedStagePaths(target, version) {
  const extension = fileExtension(target)
  return expectedNames(version).map((name) =>
    join(SRC_TAURI, "binaries", `${name}-${target}${extension}`)
  )
}

function verifyConfiguredExternalBins(target, version) {
  const config = JSON.parse(
    readFileSync(join(SRC_TAURI, "tauri.conf.json"), "utf8")
  )
  const configured = config.bundle?.externalBin ?? []
  for (const name of expectedNames(version)) {
    const expected = `binaries/${name}`
    if (!configured.includes(expected)) {
      die(`tauri.conf.json externalBin is missing ${expected}`)
    }
  }
  verifyAgentBrowserConfig(SRC_TAURI, target, die)
}

function cargoOutputPath(target) {
  return join(
    SRC_TAURI,
    "target",
    target,
    "release",
    `${BIN_NAME}${fileExtension(target)}`
  )
}

function verifyStagedSidecars(target, version) {
  const cargoPath = cargoOutputPath(target)
  logFile("Cargo MCP sidecar", cargoPath, version)
  const cargoHash = sha256(cargoPath)
  verifyConfiguredExternalBins(target, version)
  for (const stagePath of expectedStagePaths(target, version)) {
    logFile("Tauri externalBin sidecar", stagePath, version)
    if (sha256(stagePath) !== cargoHash) {
      die(`staged sidecar differs from Cargo output: ${stagePath}`)
    }
  }
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

function verifyInstalledSidecars(
  appDirectory,
  target,
  version,
  expectedHashes
) {
  const extension = fileExtension(target)
  for (const name of expectedNames(version)) {
    const path = join(appDirectory, `${name}${extension}`)
    logFile("installed MCP sidecar", path, version)
    if (expectedHashes && sha256(path) !== expectedHashes.get(name)) {
      die(`installed sidecar differs from staged externalBin source: ${path}`)
    }
  }
  verifyInstalledAgentBrowser(appDirectory, target, expectedHashes, {
    die,
    logFile,
    sha256,
  })
  verifyInstalledRuntimeSeed(appDirectory, target)
}

function verifyInstalledRuntimeSeed(appDirectory, target) {
  const seedRoot = join(appDirectory, "runtime-seed")
  if (target === "i686-pc-windows-msvc") {
    if (existsSync(seedRoot))
      die(`Windows x86 must not install runtime seed: ${seedRoot}`)
    return
  }
  const manifestPath = join(seedRoot, "manifest.json")
  requireNonEmptyFile(manifestPath, "installed runtime seed manifest")
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"))
  if (
    manifest.schemaVersion !== 2 ||
    manifest.target !== target ||
    manifest.components?.length !== 4
  ) {
    die(`installed runtime seed does not match ${target}: ${manifestPath}`)
  }
  for (const component of manifest.components) {
    const archive = resolve(seedRoot, component.archive ?? "")
    if (!archive.startsWith(`${resolve(seedRoot)}\\`))
      die(`installed runtime seed archive escaped resource root: ${archive}`)
    requireNonEmptyFile(archive, `installed ${component.id} runtime seed`)
    const metadata = lstatSync(archive)
    if (
      metadata.size !== component.archiveSize ||
      sha256(archive) !== component.archiveSha256
    ) {
      die(`installed runtime seed archive is invalid: ${archive}`)
    }
  }
  log(
    `installed runtime seed verified: target=${target} components=${manifest.components.map((item) => item.id).join(",")}`
  )
}

function stagedHashes(target, version) {
  const hashes = new Map(
    expectedNames(version).map((name, index) => [
      name,
      sha256(expectedStagePaths(target, version)[index]),
    ])
  )
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

function verifyNsisInstaller(installer, target, version) {
  if (process.platform !== "win32") die("NSIS verification requires Windows")
  logFile("NSIS installer", installer, version)
  const installRoot = mkdtempSync(join(tmpdir(), NSIS_INSTALL_PREFIX))
  try {
    log(`installing NSIS bundle into temporary root: ${installRoot}`)
    try {
      execFileSync(installer, ["/S", `/D=${installRoot}`], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      })
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
      version,
      stagedHashes(target, version)
    )
  } finally {
    rmSync(installRoot, { force: true, recursive: true })
    log(`removed temporary NSIS installation root: ${installRoot}`)
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2))
  const target = resolveTarget(args)
  const version = readVersion(args)
  if (args.installedApp) {
    if (args.installer || process.env.IYW_CLAW_VERIFY_NSIS === "1") {
      die("--installed-app cannot be combined with NSIS verification")
    }
    verifyInstalledSidecars(
      resolveInstalledApp(resolve(args.installedApp)),
      target,
      version
    )
    return
  }
  verifyStagedSidecars(target, version)
  if (args.installer || process.env.IYW_CLAW_VERIFY_NSIS === "1") {
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
