import { createHash } from "node:crypto"
import { execFile } from "node:child_process"
import { createReadStream } from "node:fs"
import {
  chmod,
  cp,
  lstat,
  mkdir,
  mkdtemp,
  readlink,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises"
import { tmpdir } from "node:os"
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from "node:path"
import { promisify } from "node:util"

const execFileAsync = promisify(execFile)
const DEFAULT_GITHUB_MIRRORS = [
  "https://gh-proxy.com",
  "https://ghfast.top",
  "https://ghproxy.net",
]

function tarExecutable() {
  if (process.platform !== "win32") return "tar"
  const windowsRoot = process.env.SystemRoot ?? process.env.WINDIR
  if (!windowsRoot) throw new Error("Windows system root is unavailable")
  return join(windowsRoot, "System32", "tar.exe")
}

function archiveTar(archive, args, options = {}) {
  return execFileAsync(tarExecutable(), args, {
    cwd: dirname(archive),
    windowsHide: true,
    ...options,
  })
}

function sha256Bytes(bytes) {
  return createHash("sha256").update(bytes).digest("hex")
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

function downloadCandidates(url) {
  if (!url.startsWith("https://github.com/")) return [url]
  const configured = process.env.IYW_CLAW_GITHUB_MIRROR?.trim()
  if (configured && /^(off|none|direct)$/i.test(configured)) return [url]
  const mirrors = configured
    ? configured.split(/[,;\s]+/).filter((item) => /^https?:\/\//.test(item))
    : DEFAULT_GITHUB_MIRRORS
  return [
    ...new Set(mirrors.map((item) => `${item.replace(/\/$/, "")}/${url}`)),
    url,
  ]
}

async function downloadArchive(spec, cacheDir) {
  const { name, expected, base } = spec
  await mkdir(cacheDir, { recursive: true })
  const cached = join(cacheDir, `${expected}-${name}`)
  try {
    if (
      (await stat(cached)).isFile() &&
      (await sha256File(cached)) === expected
    ) {
      console.log(`[runtime-seed] archive cache hit: ${name}`)
      return cached
    }
  } catch {}
  console.log(`[runtime-seed] downloading: ${name}`)
  const source = new URL(name, base).href
  let lastError
  for (const candidate of downloadCandidates(source)) {
    try {
      const response = await fetch(candidate, {
        signal: AbortSignal.timeout(180_000),
      })
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      const bytes = Buffer.from(await response.arrayBuffer())
      if (sha256Bytes(bytes) !== expected) throw new Error("SHA-256 mismatch")
      await writeFile(cached, bytes)
      return cached
    } catch (error) {
      lastError = error
      console.warn(`[runtime-seed] source failed: ${error.message}`)
    }
  }
  throw new Error(`all sources failed for ${name}: ${lastError?.message}`)
}

function safeRelativePath(value) {
  const normalized = String(value).replaceAll("\\", "/")
  return (
    normalized.length > 0 &&
    !/[\u0000-\u001f\u007f]/.test(normalized) &&
    !isAbsolute(normalized) &&
    !/^[A-Za-z]:\//.test(normalized) &&
    normalized
      .split("/")
      .every((part) => part.length > 0 && part !== "." && part !== "..")
  )
}

async function validateArchiveEntries(archive) {
  const archiveName = basename(archive)
  const { stdout } = await archiveTar(archive, ["-tf", archiveName], {
    maxBuffer: 50 * 1024 * 1024,
  })
  const seen = new Set()
  for (const raw of stdout.split(/\r?\n/)) {
    const normalized = raw.replaceAll("\\", "/")
    if (!normalized) continue
    const withoutTrailingSlash = normalized.replace(/\/+$/, "")
    if (withoutTrailingSlash === ".") continue
    const path = withoutTrailingSlash.replace(/^(?:\.\/)+/, "")
    if (!safeRelativePath(path) || seen.has(path))
      throw new Error(`archive contains unsafe or duplicate path: ${raw}`)
    seen.add(path)
  }
  if (seen.size === 0) throw new Error(`archive is empty: ${archive}`)
}

async function extractArchive(archive, destination) {
  await validateArchiveEntries(archive)
  await mkdir(destination, { recursive: true })
  await archiveTar(archive, ["-xf", basename(archive), "-C", destination])
}

function normalizedRelative(root, path) {
  return relative(root, path).split(sep).join("/")
}

async function copyTreeMaterialized(
  source,
  destination,
  extractionRoot,
  seen = new Set()
) {
  const metadata = await lstat(source)
  if (metadata.isSymbolicLink()) {
    const target = resolve(dirname(source), await readlink(source))
    const targetPath = normalizedRelative(resolve(extractionRoot), target)
    if (!safeRelativePath(targetPath))
      throw new Error(`archive link escapes extraction root: ${source}`)
    if (seen.has(target)) throw new Error(`archive link cycle: ${source}`)
    return copyTreeMaterialized(
      target,
      destination,
      extractionRoot,
      new Set([...seen, target])
    )
  }
  if (metadata.isDirectory()) {
    await mkdir(destination, { recursive: true })
    for (const entry of await readdir(source))
      await copyTreeMaterialized(
        join(source, entry),
        join(destination, entry),
        extractionRoot,
        seen
      )
    return
  }
  if (!metadata.isFile())
    throw new Error(`unsupported archive entry: ${source}`)
  await mkdir(dirname(destination), { recursive: true })
  await cp(source, destination)
  if (process.platform !== "win32")
    await chmod(destination, metadata.mode & 0o777)
}

async function findFile(root, suffix) {
  const matches = []
  await walkFiles(root, async (path, entry) => {
    if (entry.isFile() && normalizedRelative(root, path).endsWith(suffix))
      matches.push(path)
  })
  if (matches.length !== 1)
    throw new Error(`expected one ${suffix}, found ${matches.length}`)
  return matches[0]
}

async function stageArchiveComponent(componentRoot, archive, expectedPath) {
  const extraction = await mkdtemp(join(tmpdir(), "iyw-runtime-extract-"))
  try {
    await extractArchive(archive, extraction)
    const marker = await findFile(extraction, expectedPath)
    let sourceRoot = marker
    for (const _part of expectedPath.split("/").filter(Boolean))
      sourceRoot = dirname(sourceRoot)
    await copyTreeMaterialized(sourceRoot, componentRoot, extraction)
  } finally {
    await rm(extraction, { recursive: true, force: true })
  }
}

async function walkFiles(root, callback, current = root) {
  for (const entry of await readdir(current, { withFileTypes: true })) {
    const source = join(current, entry.name)
    if (entry.isDirectory()) await walkFiles(root, callback, source)
    else await callback(source, entry)
  }
}

async function buildFileManifest(root) {
  const paths = []
  await walkFiles(root, (path, entry) => {
    if (!entry.isFile()) throw new Error(`seed contains non-file: ${path}`)
    paths.push(path)
  })
  const files = await mapLimit(paths, 16, async (path) => {
    const metadata = await stat(path)
    const relativePath = normalizedRelative(root, path)
    if (!safeRelativePath(relativePath))
      throw new Error(`seed contains unsafe file path: ${relativePath}`)
    return {
      path: relativePath,
      size: metadata.size,
      sha256: await sha256File(path),
      executable: process.platform !== "win32" && (metadata.mode & 0o111) !== 0,
    }
  })
  return files.sort((left, right) => left.path.localeCompare(right.path))
}

function componentDigest(files) {
  const hash = createHash("sha256")
  for (const file of files)
    hash.update(`${file.path}\0${file.size}\0${file.sha256}\n`)
  return hash.digest("hex")
}

async function createComponentArchive(componentRoot, archivePath, files) {
  await mkdir(dirname(archivePath), { recursive: true })
  const listingRoot = await mkdtemp(join(tmpdir(), "iyw-runtime-list-"))
  const listing = join(listingRoot, "files.txt")
  try {
    await writeFile(listing, `${files.map((file) => file.path).join("\n")}\n`)
    await archiveTar(
      archivePath,
      ["-czf", basename(archivePath), "-C", componentRoot, "-T", listing],
      { maxBuffer: 20 * 1024 * 1024 }
    )
  } finally {
    await rm(listingRoot, { recursive: true, force: true })
  }
  const metadata = await stat(archivePath)
  if (!metadata.isFile() || metadata.size === 0)
    throw new Error(`component archive is empty: ${archivePath}`)
  return {
    archiveSize: metadata.size,
    archiveSha256: await sha256File(archivePath),
  }
}

export {
  archiveTar,
  buildFileManifest,
  componentDigest,
  copyTreeMaterialized,
  createComponentArchive,
  downloadArchive,
  normalizedRelative,
  safeRelativePath,
  sha256File,
  stageArchiveComponent,
}
