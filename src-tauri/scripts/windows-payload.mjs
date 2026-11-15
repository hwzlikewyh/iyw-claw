import { createHash } from "node:crypto"
import { createReadStream, existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, copyFileSync, writeFileSync } from "node:fs"
import { basename, dirname, join, relative, resolve, sep } from "node:path"
import { execFileSync } from "node:child_process"
import { fileURLToPath } from "node:url"

const TARGET = "x86_64-pc-windows-msvc"
const MAX_FILES = 100000
const MAX_BYTES = 4 * 1024 ** 3
const SCRIPT = fileURLToPath(import.meta.url)

export function safePayloadPath(value) {
  if (typeof value !== "string" || /[\\:\x00-\x1f<>"|?*]/.test(value)) throw new Error("unsafe payload path")
  const parts = value.split("/")
  if (parts.some(p => !p || p === "." || p === ".." || /[. ]$/.test(p) || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(p)))
    throw new Error("unsafe payload path segment")
  return value
}

async function digest(file) {
  const hash = createHash("sha256")
  for await (const block of createReadStream(file)) hash.update(block)
  return hash.digest("hex")
}

function collect(source, destination, entries) {
  const stat = lstatSync(source)
  if (stat.isSymbolicLink()) throw new Error("payload source cannot be a link")
  if (stat.isDirectory()) {
    for (const name of readdirSync(source).sort()) collect(join(source, name), `${destination}/${name}`, entries)
  } else if (stat.isFile()) {
    entries.push({ source, path: safePayloadPath(destination), size: stat.size })
  } else throw new Error("payload source must be a regular file")
}

export async function stagePayload({ root, output, version, sourceCommit, installer, webview }) {
  const config = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"))
  if (config.version !== version || !/^\d+\.\d+\.\d+$/.test(version) || !/^[a-f0-9]{40}$/.test(sourceCommit))
    throw new Error("payload version or source identity mismatch")
  if (existsSync(output)) throw new Error("payload stage must be a new directory")
  const entries = []
  collect(join(root, `src-tauri/target/${TARGET}/release/iyw-claw.exe`), "app/iyw-claw.exe", entries)
  for (const bin of config.bundle.externalBin) {
    collect(join(root, "src-tauri", `${bin}-${TARGET}.exe`), `app/${basename(bin)}.exe`, entries)
  }
  for (const [source, destination] of Object.entries(config.bundle.resources)) {
    collect(resolve(root, "src-tauri", source), `app/${safePayloadPath(destination.replace(/\/$/, ""))}`, entries)
  }
  collect(installer, "app/installer/原助理安装向导.exe", entries)
  collect(webview, "app/installer/webview2-setup.exe", entries)
  const names = new Set()
  let total = 0
  for (const entry of entries) {
    const key = entry.path.toLowerCase()
    if (names.has(key)) throw new Error("case-insensitive duplicate payload path")
    names.add(key)
    total += entry.size
  }
  if (entries.length > MAX_FILES || total > MAX_BYTES) throw new Error("payload exceeds safety limits")
  for (const required of ["app/iyw-claw.exe", "app/iyw-environment.exe", "app/resources/bundle/index.html", "app/xinghe-resources/runtime.json", "app/native-share/config.json"])
    if (!names.has(required)) throw new Error(`payload missing ${required}`)
  mkdirSync(output, { recursive: true })
  const files = []
  for (const entry of entries.sort((a, b) => a.path < b.path ? -1 : 1)) {
    const target = join(output, entry.path)
    mkdirSync(dirname(target), { recursive: true })
    copyFileSync(entry.source, target)
    files.push({ path: entry.path, size: entry.size, sha256: await digest(target) })
  }
  const manifest = { schemaVersion: 1, product: "iyw-claw", version, sourceCommit, target: "windows", arch: "x86_64", files }
  writeFileSync(join(output, "payload-manifest.json"), JSON.stringify(manifest) + "\n")
  return manifest
}

export async function packageWindowsPayload(context) {
  const manifest = await stagePayload(context)
  const archive = join(dirname(context.output), `iyw-claw_${context.version}_x64-app-payload.zip`)
  execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
    join(dirname(SCRIPT), "windows-payload-zip.ps1"), "-Source", context.output, "-Destination", archive], { stdio: "inherit", windowsHide: true })
  console.log(`[windows-payload] packaged ${manifest.files.length} files from ${manifest.sourceCommit}`)
  return archive
}
