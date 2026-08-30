import { lstat, mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises"
import { dirname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { contracts } from "../shared/contracts-data.mjs"

const root = fileURLToPath(new URL("..", import.meta.url))
const manifest = JSON.parse(await readFile(join(root, ".iyw-plugin.json"), "utf8"))
if (manifest.schemaVersion !== 2 || manifest.name !== "infinite-canvas" || manifest.version !== "0.1.11" || JSON.stringify(manifest.targets) !== JSON.stringify(["iyw-claw"])) throw new Error("plugin manifest identity is invalid")
const upstream = JSON.parse(await readFile(join(root, "upstream.json"), "utf8"))
if (upstream.license !== "MIT" || typeof upstream.commit !== "string" || !/^[a-f0-9]{40}$/.test(upstream.commit)) throw new Error("upstream license provenance is invalid")
if (manifest.permissions.network.connectDomains.length || manifest.permissions.network.resourceDomains.length || manifest.permissions.network.frameDomains.length || (manifest.permissions.network.baseUriDomains || []).length) throw new Error("network permissions must be empty")
const schemaFiles = new Set(Object.values(contracts).map((value) => value.schemaPath))
for (const [name, value] of Object.entries(contracts)) {
  const path = join(root, value.schemaPath)
  const actual = JSON.parse(await readFile(path, "utf8"))
  if (JSON.stringify(actual) !== JSON.stringify(value.inputSchema)) throw new Error(`schema drift: ${name}`)
}
if (schemaFiles.size !== 10) throw new Error(`expected 10 schemas, got ${schemaFiles.size}`)
const widget = await stat(join(root, "widget/dist/infinite-canvas-widget.html"))
if (widget.size > 8 * 1024 * 1024) throw new Error("widget exceeds 8 MiB")
await stat(join(root, "runtime/dist/infinite-canvas-mcp.mjs"))
const licenseReport = await buildLicenseReport()
const creativeSource = await readFile(join(root, "widget/src/creative-request.ts"), "utf8")
for (const action of ["image.generate", "image.annotation-edit", "html.generate", "html.edit", "slides.generate", "slides.annotation-edit"]) if (!creativeSource.includes(action)) throw new Error(`missing creative action: ${action}`)
const registrySource = await readFile(join(root, "widget/src/plugins/register-builtins.ts"), "utf8")
for (const plugin of ["annotationPlugin", "htmlPlugin", "markdownPlugin", "slidesPlugin", "svgPlugin"]) if (!registrySource.includes(plugin)) throw new Error(`missing builtin plugin: ${plugin}`)
const files = await walk(root)
if (files.length > 512) throw new Error("plugin file count exceeds 512")
const expanded = (await Promise.all(files.map(async (path) => (await stat(path)).size))).reduce((sum, size) => sum + size, 0)
if (expanded > 50 * 1024 * 1024) throw new Error("plugin expanded size exceeds 50 MiB")
const forbidden = /tldraw|canvas\.best|agentToken/i
for (const path of files) {
  const name = relative(root, path).replaceAll("\\", "/")
  if (name.includes(".codex-plugin") || name.includes(".claude-plugin") || name === ".mcp.json") throw new Error(`forbidden native artifact: ${name}`)
  if (name !== "upstream.json" && !name.startsWith("scripts/") && /\.(mjs|json|md|html|ts)$/.test(name) && forbidden.test(await readFile(path, "utf8"))) throw new Error(`forbidden text in ${name}`)
}
const receipt = { schemaVersion: 1, verifiedAt: new Date().toISOString(), tools: Object.keys(contracts), resources: ["ui://widget/infinite-canvas/canvas.html"], files: files.length, expandedBytes: expanded, widgetBytes: widget.size }
await mkdir(join(root, "dist"), { recursive: true })
await writeFile(join(root, "dist/license-report.json"), `${JSON.stringify(licenseReport, null, 2)}\n`, "utf8")
await writeFile(join(root, "dist/verify-receipt.json"), `${JSON.stringify(receipt, null, 2)}\n`, "utf8")
console.log(JSON.stringify(receipt, null, 2))

async function walk(directory) {
  const result = []
  for (const entry of await (await import("node:fs/promises")).readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    const info = await lstat(path)
    if (info.isSymbolicLink()) throw new Error(`symbolic link is not allowed: ${relative(root, path)}`)
    if (entry.isDirectory() && !["vendor", "dist", "node_modules"].includes(entry.name)) result.push(...await walk(path))
    else if (entry.isFile() && !path.includes(`${join(root, "node_modules")}`)) result.push(path)
  }
  return result
}

async function buildLicenseReport() {
  const packageJson = JSON.parse(await readFile(join(root, "package.json"), "utf8"))
  const packageIndex = await loadPackageIndex()
  const packages = new Map()
  const pending = Object.keys(packageJson.dependencies || {}).map((name) => ({ name, base: root }))
  while (pending.length) {
    const request = pending.pop()
    if (!request) continue
    const path = join(request.base, "node_modules", ...request.name.split("/"), "package.json")
    let value
    try { value = JSON.parse(await readFile(path, "utf8")) } catch { value = packageIndex.get(request.name) }
    if (!value) continue
    const key = `${value.name || request.name}@${value.version}`
    if (packages.has(key)) continue
    packages.set(key, { name: value.name || request.name, version: value.version, license: typeof value.license === "string" ? value.license : "UNKNOWN" })
    const base = dirname(path)
    pending.push(...Object.keys(value.dependencies || {}).map((name) => ({ name, base })), ...Object.keys(value.optionalDependencies || {}).map((name) => ({ name, base })))
  }
  const report = {}
  for (const value of [...packages.values()].sort((left, right) => left.name.localeCompare(right.name))) (report[value.license] ||= []).push(value)
  const allowed = new Set(["MIT", "ISC", "BSD-2-Clause", "BSD-3-Clause", "Apache-2.0"])
  const unsupported = Object.keys(report).filter((license) => !allowed.has(license))
  if (unsupported.length) throw new Error(`unsupported production license: ${unsupported.join(", ")}`)
  return Object.fromEntries(Object.entries(report).sort(([left], [right]) => left.localeCompare(right)))
}

async function loadPackageIndex() {
  const result = new Map()
  const store = join(root, "node_modules", ".pnpm")
  const entries = (await readdir(store, { withFileTypes: true })).filter((entry) => entry.isDirectory()).sort((left, right) => left.name.localeCompare(right.name))
  for (const entry of entries) {
    const modules = join(store, entry.name, "node_modules")
    for (const module of await readdir(modules, { withFileTypes: true }).catch(() => [])) {
      const candidates = module.name.startsWith("@") ? await readdir(join(modules, module.name), { withFileTypes: true }).catch(() => []) : [module]
      for (const candidate of candidates) {
        const packagePath = module.name.startsWith("@") ? join(modules, module.name, candidate.name, "package.json") : join(modules, candidate.name, "package.json")
        try { const value = JSON.parse(await readFile(packagePath, "utf8")); if (!result.has(value.name)) result.set(value.name, value) } catch { continue }
      }
    }
  }
  return result
}
