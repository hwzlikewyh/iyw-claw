import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
  writeFile,
  rename,
} from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { execFile } from "node:child_process"
import { promisify } from "node:util"
import {
  DOWNLOADS,
  TARGETS,
  parseTarget,
  targetInfo,
} from "./runtime-seed-config.mjs"
import {
  buildFileManifest,
  componentDigest,
  copyTreeMaterialized,
  createComponentArchive,
  downloadArchive,
  normalizedRelative,
  safeRelativePath,
  sha256File,
  stageArchiveComponent,
} from "./runtime-seed-files.mjs"

const execFileAsync = promisify(execFile)
const ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)))
const SEED_ROOT = join(ROOT, "src-tauri", "resources", "runtime-seed")
const OVERLAY_PATH = join(ROOT, "src-tauri", "tauri.runtime-seed.conf.json")
const CODEX_LOCK_ROOT = join(ROOT, "src-tauri", "runtime-seed", "codex")
async function packageVersion() {
  const packageJson = JSON.parse(
    await readFile(join(ROOT, "package.json"), "utf8")
  )
  return packageJson.version
}

function codexNpmInvocation(staging, info) {
  const args = [
    "ci",
    "--prefix",
    staging,
    "--include=optional",
    "--omit=dev",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    `--os=${info.npm[0]}`,
    `--cpu=${info.npm[1]}`,
  ]
  const npmCli = join(
    dirname(process.execPath),
    "node_modules",
    "npm",
    "bin",
    "npm-cli.js"
  )
  return process.platform === "win32"
    ? [process.execPath, [npmCli, ...args]]
    : ["npm", args]
}

async function validateCodexPrefix(staging, info, spec) {
  const packagePath = join(
    staging,
    "node_modules",
    "@agentclientprotocol",
    "codex-acp",
    "package.json"
  )
  const packageJson = JSON.parse(await readFile(packagePath, "utf8"))
  if (packageJson.version !== spec.version)
    throw new Error(`Codex ACP version is not pinned to ${spec.version}`)
  await stat(
    join(
      staging,
      "node_modules",
      "@openai",
      `codex-${info.npm[0]}-${info.npm[1]}`,
      "package.json"
    )
  )
  const [, entrypoint] = componentEntrypoints("codex-acp", info).at(-1)
  await stat(join(staging, entrypoint))
}

async function prepareCodex(componentRoot, info, cacheDir) {
  const spec = DOWNLOADS["codex-acp"]
  const staging = await mkdtemp(join(tmpdir(), "iyw-codex-prefix-"))
  try {
    await Promise.all(
      ["package.json", "package-lock.json"].map((name) =>
        copyFile(join(CODEX_LOCK_ROOT, name), join(staging, name))
      )
    )
    const npm = codexNpmInvocation(staging, info)
    const env = {
      ...process.env,
      npm_config_cache: join(cacheDir, "npm"),
    }
    await execFileAsync(npm[0], npm[1], {
      cwd: ROOT,
      env,
      windowsHide: true,
      maxBuffer: 20 * 1024 * 1024,
    })
    await validateCodexPrefix(staging, info, spec)
    await copyTreeMaterialized(staging, componentRoot, staging)
  } finally {
    await rm(staging, { recursive: true, force: true })
  }
}

function componentEntrypoints(id, info) {
  if (id === "node")
    return [
      ["node", info.os === "windows" ? "node.exe" : "bin/node"],
      ["npm", info.os === "windows" ? "npm.cmd" : "bin/npm"],
    ]
  if (id === "uv")
    return [
      ["uv", info.os === "windows" ? "uv.exe" : "uv"],
      ["uvx", info.os === "windows" ? "uvx.exe" : "uvx"],
    ]
  if (id === "git")
    return [["git", info.os === "windows" ? "cmd/git.exe" : "bin/git"]]
  return [
    [
      "codex-acp",
      info.os === "windows"
        ? "node_modules/.bin/codex-acp.cmd"
        : "node_modules/.bin/codex-acp",
    ],
  ]
}

function sourceMarker(id, info) {
  if (id === "node") return info.os === "windows" ? "node.exe" : "bin/node"
  if (id === "uv") return info.os === "windows" ? "uv.exe" : "uv"
  return info.os === "windows" ? "cmd/git.exe" : "bin/git"
}

async function prepareDownloadedComponent(id, info, root, cacheDir) {
  const spec = DOWNLOADS[id]
  const [name, expected] = spec[info.platform]
  const base =
    id === "git" && info.os !== "windows" ? spec.nonWindowsBase : spec.base
  const archive = await downloadArchive({ name, expected, base }, cacheDir)
  await stageArchiveComponent(root, archive, sourceMarker(id, info))
  if (expected !== (await sha256File(archive)))
    throw new Error(`archive checksum changed during staging: ${name}`)
}

async function prepareComponent(id, info, target, seedRoot, cacheDir) {
  const spec = DOWNLOADS[id]
  const version =
    id === "git" && info.os !== "windows"
      ? spec.nonWindowsVersion
      : spec.version
  const componentRoot = join(seedRoot, ".component-staging", id)
  await mkdir(componentRoot, { recursive: true })
  if (id === "codex-acp") await prepareCodex(componentRoot, info, cacheDir)
  else await prepareDownloadedComponent(id, info, componentRoot, cacheDir)
  const files = await buildFileManifest(componentRoot)
  const entries = componentEntrypoints(id, info)
  for (const [, path] of entries) {
    if (!safeRelativePath(path) || !files.some((file) => file.path === path))
      throw new Error(`missing ${id} entrypoint: ${path}`)
  }
  const entrypoints = Object.fromEntries(entries)
  for (const path of Object.values(entrypoints)) {
    if (!safeRelativePath(path) || !files.some((file) => file.path === path))
      throw new Error(`missing ${id} entrypoint: ${path}`)
  }
  const archivePath = join(seedRoot, "components", `${id}-${target}.tar.gz`)
  const archiveMetadata = await createComponentArchive(
    componentRoot,
    archivePath,
    files
  )
  await rm(componentRoot, { recursive: true, force: true })
  return {
    id,
    kind: id === "codex-acp" ? "npm_agent" : "runtime_tool",
    version,
    archive: normalizedRelative(seedRoot, archivePath),
    ...archiveMetadata,
    sha256: componentDigest(files),
    totalSize: files.reduce((sum, file) => sum + file.size, 0),
    entrypoints,
    files,
  }
}

async function prepareComponents(info, target, staging, cacheDir) {
  const components = []
  for (const id of ["node", "git", "uv", "codex-acp"]) {
    console.log(`[runtime-seed] preparing component: ${id}`)
    const component = await prepareComponent(
      id,
      info,
      target,
      staging,
      cacheDir
    )
    components.push(component)
    console.log(`[runtime-seed] component ready: ${id}`)
  }
  return components
}

async function writeSeedManifest(staging, target, info, components) {
  const manifest = {
    schemaVersion: 2,
    createdBy: "iyw-runtime-seed-builder",
    appVersion: await packageVersion(),
    target,
    arch: info.arch,
    platform: info.platform,
    components,
  }
  await writeFile(
    join(staging, "manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`
  )
}

async function writeOverlay(enabled) {
  if (!enabled) return rm(OVERLAY_PATH, { force: true })
  await writeFile(
    OVERLAY_PATH,
    `${JSON.stringify({ bundle: { resources: { "resources/runtime-seed": "runtime-seed" } } }, null, 2)}\n`
  )
}

export { TARGETS, targetInfo, parseTarget }

export async function prepareRuntimeSeed(target = parseTarget()) {
  if (!target)
    throw new Error("runtime seed target is required; pass --target <triple>")
  const info = targetInfo(target)
  await rm(SEED_ROOT, { recursive: true, force: true })
  await writeOverlay(!info.skipped)
  if (info.skipped) {
    console.log(
      `[runtime-seed] ${target} is Windows x86; keeping online Version Center path`
    )
    return
  }
  const cacheDir = resolve(
    process.env.IYW_CLAW_RUNTIME_SEED_CACHE ?? join(ROOT, ".runtime-seed-cache")
  )
  await mkdir(dirname(SEED_ROOT), { recursive: true })
  const staging = await mkdtemp(
    join(dirname(SEED_ROOT), "runtime-seed-staging-")
  )
  try {
    const components = await prepareComponents(info, target, staging, cacheDir)
    await rm(join(staging, ".component-staging"), {
      recursive: true,
      force: true,
    })
    await writeSeedManifest(staging, target, info, components)
    await rename(staging, SEED_ROOT)
    console.log(
      `[runtime-seed] prepared ${target}: ${components.map((component) => component.id).join(", ")}`
    )
  } catch (error) {
    await rm(staging, { recursive: true, force: true })
    throw error
  }
}

const entry =
  process.argv[1] &&
  resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
if (entry)
  prepareRuntimeSeed().catch((error) => {
    console.error(`[runtime-seed] ${error.message}`)
    process.exitCode = 1
  })
