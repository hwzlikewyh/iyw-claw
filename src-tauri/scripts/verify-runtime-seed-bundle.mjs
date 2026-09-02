#!/usr/bin/env node

import { createHash } from "node:crypto"
import { createReadStream } from "node:fs"
import { execFile } from "node:child_process"
import { mkdtemp, readdir, readFile, rm, stat } from "node:fs/promises"
import { tmpdir } from "node:os"
import { isAbsolute, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { promisify } from "node:util"

import { parseTarget, targetInfo } from "./runtime-seed-config.mjs"

const ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)))
const COMPONENT_IDS = new Set(["node", "git", "uv", "codex-acp"])
const execFileAsync = promisify(execFile)

function fail(message) {
  throw new Error(message)
}

function parseArgs(argv) {
  const targetIndex = argv.indexOf("--target")
  const target = targetIndex >= 0 ? argv[targetIndex + 1] : parseTarget()
  if (!target || target.startsWith("--"))
    fail("runtime seed bundle target is required; pass --target <triple>")
  if (
    argv.some(
      (arg, index) =>
        arg.startsWith("--") && arg !== "--target" && index !== targetIndex
    )
  )
    fail("unknown argument")
  return target
}

async function findApp(root) {
  const entries = await readdir(root, { withFileTypes: true })
  for (const entry of entries) {
    const path = join(root, entry.name)
    if (entry.isDirectory() && entry.name.endsWith(".app")) return path
    if (entry.isDirectory()) {
      const found = await findApp(path)
      if (found) return found
    }
  }
  return null
}

async function sha256File(path) {
  const hash = createHash("sha256")
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest("hex")
}

async function requireArchive(seedRoot, component) {
  const archivePath = String(component.archive ?? "").replaceAll("\\", "/")
  const archiveParts = archivePath.split("/")
  if (
    !archivePath.startsWith("components/") ||
    !archivePath.endsWith(".tar.gz") ||
    archiveParts.some((part) => !part || part === "." || part === "..")
  ) {
    fail(`invalid runtime seed archive path: ${component.id}`)
  }
  const archive = resolve(seedRoot, archivePath)
  const archiveRelative = relative(seedRoot, archive)
  if (
    !archiveRelative ||
    isAbsolute(archiveRelative) ||
    archiveRelative.startsWith("..")
  )
    fail(`runtime seed archive escaped resource root: ${component.id}`)
  let metadata
  try {
    metadata = await stat(archive)
  } catch {
    fail(`runtime seed archive is missing: ${component.id}`)
  }
  if (!metadata.isFile() || metadata.size !== component.archiveSize)
    fail(`runtime seed archive size or type mismatch: ${component.id}`)
  const actual = await sha256File(archive)
  if (actual.toLowerCase() !== String(component.archiveSha256).toLowerCase())
    fail(`runtime seed archive SHA-256 mismatch: ${component.id}`)
}

async function verifyApp(appDirectory, target, info) {
  const seedRoot = join(appDirectory, "Contents", "Resources", "runtime-seed")
  const manifestPath = join(seedRoot, "manifest.json")
  let manifest
  try {
    manifest = JSON.parse(await readFile(manifestPath, "utf8"))
  } catch {
    fail(`runtime seed manifest is missing or invalid: ${manifestPath}`)
  }
  const packageJson = JSON.parse(
    await readFile(join(ROOT, "package.json"), "utf8")
  )
  if (
    manifest.schemaVersion !== 2 ||
    manifest.createdBy !== "iyw-runtime-seed-builder" ||
    manifest.appVersion !== packageJson.version ||
    manifest.target !== target ||
    manifest.arch !== info.arch ||
    manifest.platform !== info.platform
  ) {
    fail(`runtime seed identity does not match ${target}`)
  }
  if (
    !Array.isArray(manifest.components) ||
    manifest.components.length !== COMPONENT_IDS.size ||
    new Set(manifest.components.map((component) => component.id)).size !==
      COMPONENT_IDS.size ||
    manifest.components.some((component) => !COMPONENT_IDS.has(component.id))
  ) {
    fail(`runtime seed component set is incomplete: ${manifestPath}`)
  }
  for (const component of manifest.components)
    await requireArchive(seedRoot, component)
  console.log(`[runtime-seed-bundle] verified ${target}: ${appDirectory}`)
}

async function verifyDmg(dmgPath, target, info) {
  const mountPoint = await mkdtemp(join(tmpdir(), "iyw-runtime-seed-dmg-"))
  let mounted = false
  try {
    await execFileAsync("hdiutil", [
      "attach",
      dmgPath,
      "-readonly",
      "-nobrowse",
      "-mountpoint",
      mountPoint,
    ])
    mounted = true
    const appDirectory = await findApp(mountPoint)
    if (!appDirectory) fail(`macOS app is missing from DMG: ${dmgPath}`)
    await verifyApp(appDirectory, target, info)
    console.log(`[runtime-seed-bundle] verified DMG: ${dmgPath}`)
  } finally {
    if (mounted) {
      await execFileAsync("hdiutil", ["detach", mountPoint, "-force"]).catch(
        () => undefined
      )
    }
    await rm(mountPoint, { recursive: true, force: true })
  }
}

async function verifyRuntimeSeedBundle(target = parseTarget()) {
  const info = targetInfo(target)
  if (info.skipped)
    fail(`runtime seed bundle verification is not for ${target}`)
  const bundleRoot = join(
    ROOT,
    "src-tauri",
    "target",
    target,
    "release",
    "bundle"
  )
  const appDirectory = await findApp(bundleRoot)
  if (!appDirectory) fail(`macOS app bundle is missing under ${bundleRoot}`)
  await verifyApp(appDirectory, target, info)
  const artifacts = await readdir(bundleRoot, { recursive: true })
  const dmgPaths = artifacts
    .filter((path) => typeof path === "string" && path.endsWith(".dmg"))
    .map((path) => join(bundleRoot, path))
  if (dmgPaths.length === 0) fail(`macOS DMG is missing under ${bundleRoot}`)
  for (const dmgPath of dmgPaths) await verifyDmg(dmgPath, target, info)
}

const entry =
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
if (entry) {
  verifyRuntimeSeedBundle(parseArgs(process.argv.slice(2))).catch((error) => {
    console.error(`[runtime-seed-bundle] ${error.message}`)
    process.exitCode = 1
  })
}

export { verifyRuntimeSeedBundle }
