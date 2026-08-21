import { createHash } from "node:crypto"
import { createReadStream } from "node:fs"
import { lstat, mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises"
import { tmpdir } from "node:os"
import { basename, join, relative, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import { parseTarget, targetInfo } from "./runtime-seed-config.mjs"
import { archiveTar, safeRelativePath } from "./runtime-seed-files.mjs"

const ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)))
const SEED_ROOT = join(ROOT, "src-tauri", "resources", "runtime-seed")
const OVERLAY_PATH = join(ROOT, "src-tauri", "tauri.runtime-seed.conf.json")
const COMPONENT_KINDS = {
  node: "runtime_tool",
  git: "runtime_tool",
  uv: "runtime_tool",
  "codex-acp": "npm_agent",
}

async function sha256File(path) {
  const hash = createHash("sha256")
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest("hex")
}

async function mapLimit(items, limit, mapper) {
  const results = new Array(items.length)
  let cursor = 0
  async function worker() {
    while (cursor < items.length) {
      const index = cursor
      cursor += 1
      results[index] = await mapper(items[index], index)
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(limit, items.length) }, worker)
  )
  return results
}

function normalizedRelative(root, path) {
  return relative(root, path).split(sep).join("/")
}

async function collectFiles(root, current, result) {
  for (const entry of await readdir(current, { withFileTypes: true })) {
    const path = join(current, entry.name)
    const metadata = await lstat(path)
    if (metadata.isSymbolicLink())
      throw new Error(
        `seed contains symbolic link: ${normalizedRelative(root, path)}`
      )
    if (metadata.isDirectory()) await collectFiles(root, path, result)
    else if (metadata.isFile()) result.push({ path, metadata })
    else
      throw new Error(
        `seed contains unsupported entry: ${normalizedRelative(root, path)}`
      )
  }
}

async function listFiles(root) {
  const entries = []
  await collectFiles(root, root, entries)
  const result = await mapLimit(entries, 16, async ({ path, metadata }) => ({
    path: normalizedRelative(root, path),
    size: metadata.size,
    sha256: await sha256File(path),
    executable: process.platform !== "win32" && (metadata.mode & 0o111) !== 0,
  }))
  return result.sort((left, right) => left.path.localeCompare(right.path))
}

function componentDigest(files) {
  const hash = createHash("sha256")
  for (const file of files)
    hash.update(`${file.path}\0${file.size}\0${file.sha256}\n`)
  return hash.digest("hex")
}

function normalizeArchivePath(value) {
  return value
    .replaceAll("\\", "/")
    .replace(/^(\.\/)+/, "")
    .replace(/\/+$/, "")
}

function expectedDirectories(files) {
  const directories = new Set()
  for (const file of files) {
    const parts = file.path.split("/")
    for (let count = 1; count < parts.length; count += 1)
      directories.add(parts.slice(0, count).join("/"))
  }
  return directories
}

async function inspectArchive(archive, files) {
  const options = { maxBuffer: 50 * 1024 * 1024 }
  const archiveName = basename(archive)
  const [{ stdout: names }, { stdout: details }] = await Promise.all([
    archiveTar(archive, ["-tf", archiveName], options),
    archiveTar(archive, ["-tvf", archiveName], options),
  ])
  const paths = names.split(/\r?\n/).filter(Boolean)
  const types = details.split(/\r?\n/).filter(Boolean)
  if (paths.length !== types.length)
    throw new Error(`archive listing is inconsistent: ${archive}`)
  const expectedFiles = new Set(files.map((file) => file.path))
  const expectedDirs = expectedDirectories(files)
  const seen = new Set()
  for (let index = 0; index < paths.length; index += 1) {
    const path = normalizeArchivePath(paths[index])
    const type = types[index][0]
    if (!path) {
      if (type !== "d") throw new Error(`archive has an empty file path`)
      continue
    }
    if (!safeRelativePath(path) || seen.has(path))
      throw new Error(`archive has an unsafe or duplicate path: ${path}`)
    if (type === "-" && !expectedFiles.has(path))
      throw new Error(`archive has an unlisted file: ${path}`)
    if (type === "d" && !expectedDirs.has(path))
      throw new Error(`archive has an unlisted directory: ${path}`)
    if (type !== "-" && type !== "d")
      throw new Error(`archive has a link or unsupported entry: ${path}`)
    seen.add(path)
  }
  if ([...expectedFiles].some((path) => !seen.has(path)))
    throw new Error(`archive is missing declared files: ${archive}`)
}

async function extractedFileManifest(archive) {
  const destination = await mkdtemp(join(tmpdir(), "iyw-seed-verify-"))
  try {
    await archiveTar(archive, ["-xf", basename(archive), "-C", destination], {
      maxBuffer: 20 * 1024 * 1024,
    })
    return await listFiles(destination)
  } finally {
    await rm(destination, { recursive: true, force: true })
  }
}

function expectedEntrypoints(id, platform) {
  const windows = platform.startsWith("win-")
  if (id === "node")
    return {
      node: windows ? "node.exe" : "bin/node",
      npm: windows ? "npm.cmd" : "bin/npm",
    }
  if (id === "git") return { git: windows ? "cmd/git.exe" : "bin/git" }
  if (id === "uv")
    return { uv: windows ? "uv.exe" : "uv", uvx: windows ? "uvx.exe" : "uvx" }
  return {
    "codex-acp": windows
      ? "node_modules/.bin/codex-acp.cmd"
      : "node_modules/.bin/codex-acp",
  }
}

async function verifyComponent(seedRoot, component, platform) {
  if (COMPONENT_KINDS[component.id] !== component.kind)
    throw new Error(`invalid component kind: ${component.id}`)
  if (
    !safeRelativePath(component.archive) ||
    !component.archive.startsWith("components/") ||
    !component.archive.endsWith(".tar.gz")
  )
    throw new Error(`invalid component archive: ${component.id}`)
  if (!Array.isArray(component.files) || component.files.length === 0)
    throw new Error(`component has no files: ${component.id}`)
  const declared = component.files
    .map(({ path, size, sha256, executable }) => ({
      path,
      size,
      sha256: sha256.toLowerCase(),
      executable: Boolean(executable),
    }))
    .sort((left, right) => left.path.localeCompare(right.path))
  const archive = join(seedRoot, component.archive)
  const metadata = await lstat(archive)
  if (!metadata.isFile() || metadata.size !== component.archiveSize)
    throw new Error(`archive size or type mismatch: ${component.id}`)
  if ((await sha256File(archive)) !== component.archiveSha256.toLowerCase())
    throw new Error(`archive SHA-256 mismatch: ${component.id}`)
  await inspectArchive(archive, declared)
  const rootFiles = await extractedFileManifest(archive)
  if (JSON.stringify(rootFiles) !== JSON.stringify(declared))
    throw new Error(`file set or digest mismatch: ${component.id}`)
  if (
    component.totalSize !== declared.reduce((sum, file) => sum + file.size, 0)
  )
    throw new Error(`totalSize mismatch: ${component.id}`)
  if (component.sha256.toLowerCase() !== componentDigest(declared))
    throw new Error(`component SHA-256 mismatch: ${component.id}`)
  const expected = expectedEntrypoints(component.id, platform)
  if (JSON.stringify(component.entrypoints) !== JSON.stringify(expected))
    throw new Error(`entrypoint map mismatch: ${component.id}`)
  for (const path of Object.values(expected)) {
    if (!safeRelativePath(path) || !declared.some((file) => file.path === path))
      throw new Error(`missing entrypoint: ${component.id}/${path}`)
  }
}

async function verifyOverlay() {
  const overlay = JSON.parse(await readFile(OVERLAY_PATH, "utf8"))
  const resources = overlay.bundle?.resources
  if (
    resources?.["../out"] ||
    resources?.["resources/runtime-seed"] !== "runtime-seed"
  ) {
    throw new Error(
      "runtime seed overlay must preserve frontend resources and map only runtime-seed"
    )
  }
}

async function verifySkippedTarget(target) {
  for (const path of [SEED_ROOT, OVERLAY_PATH]) {
    try {
      await stat(path)
      throw new Error(`Windows x86 must not contain runtime seed: ${path}`)
    } catch (error) {
      if (error.code !== "ENOENT") throw error
    }
  }
  console.log(`[runtime-seed] verified skipped target ${target}`)
}

async function readAndValidateManifest(target, info) {
  const manifest = JSON.parse(
    await readFile(join(SEED_ROOT, "manifest.json"), "utf8")
  )
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
  )
    throw new Error(
      "runtime seed identity does not match target or app version"
    )
  const components = manifest.components
  if (!Array.isArray(components))
    throw new Error("runtime seed components are invalid")
  const ids = components.map((component) => component.id)
  if (
    components.length !== 4 ||
    new Set(ids).size !== 4 ||
    ids.some((id) => !COMPONENT_KINDS[id])
  )
    throw new Error("runtime seed component set is incomplete")
  return components
}

async function verifyRuntimeSeed(target = parseTarget()) {
  if (!target)
    throw new Error("runtime seed target is required; pass --target <triple>")
  const info = targetInfo(target)
  if (info.skipped) return verifySkippedTarget(target)
  await verifyOverlay()
  const components = await readAndValidateManifest(target, info)
  for (const component of components)
    await verifyComponent(SEED_ROOT, component, info.platform)
  console.log(
    `[runtime-seed] verified ${target}: ${components.map((component) => component.id).join(", ")}`
  )
}

const entry =
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
if (entry)
  verifyRuntimeSeed().catch((error) => {
    console.error(`[runtime-seed] ${error.message}`)
    process.exitCode = 1
  })

export { verifyRuntimeSeed }
