import { lstat, mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises"
import { dirname, join, relative, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { isDeepStrictEqual } from "node:util"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js"
import { contracts } from "../shared/contracts-data.mjs"

const root = fileURLToPath(new URL("..", import.meta.url))
const manifest = JSON.parse(await readFile(join(root, ".iyw-plugin.json"), "utf8"))
if (manifest.schemaVersion !== 2 || manifest.name !== "infinite-canvas" || manifest.version !== "0.1.22" || JSON.stringify(manifest.targets) !== JSON.stringify(["iyw-claw"])) throw new Error("plugin manifest identity is invalid")
if (manifest.minClientVersion !== "0.1.332") throw new Error("interactive plugin minimum client version is invalid")
const upstream = JSON.parse(await readFile(join(root, "upstream.json"), "utf8"))
if (upstream.license !== "MIT" || typeof upstream.commit !== "string" || !/^[a-f0-9]{40}$/.test(upstream.commit)) throw new Error("upstream license provenance is invalid")
if (JSON.stringify(manifest.permissions.network.connectDomains) !== JSON.stringify(["https://raw.githubusercontent.com", "https://vol-ai.iywtu.com"]) || JSON.stringify(manifest.permissions.network.resourceDomains) !== JSON.stringify(["https://raw.githubusercontent.com"]) || manifest.permissions.network.frameDomains.length || (manifest.permissions.network.baseUriDomains || []).length) throw new Error("plugin network permissions are invalid")
const schemaFiles = new Set(Object.values(contracts).map((value) => value.schemaPath))
for (const [name, value] of Object.entries(contracts)) {
  const path = join(root, value.schemaPath)
  const actual = JSON.parse(await readFile(path, "utf8"))
  if (JSON.stringify(actual) !== JSON.stringify(value.inputSchema)) throw new Error(`schema drift: ${name}`)
}
const capabilities = manifest.components.capabilities
if (schemaFiles.size !== capabilities.length) throw new Error("manifest and runtime tool counts differ")
for (const capability of capabilities) {
  const contract = contracts[capability.toolName]
  if (!contract || contract.schemaPath !== capability.schemaPath) throw new Error(`missing runtime contract: ${capability.toolName}`)
}
const widget = await stat(join(root, "widget/dist/infinite-canvas-widget.html"))
if (widget.size > 8 * 1024 * 1024) throw new Error("widget exceeds 8 MiB")
await stat(join(root, "runtime/dist/infinite-canvas-mcp.mjs"))
await verifyRuntime()
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

async function verifyRuntime() {
  const client = new Client({ name: "infinite-canvas-package-verifier", version: manifest.version })
  const transport = new StdioClientTransport({ command: process.execPath, args: [join(root, "runtime/dist/infinite-canvas-mcp.mjs")] })
  try {
    await client.connect(transport)
    if (client.getServerVersion()?.version !== manifest.version) throw new Error("runtime version differs from manifest")
    const { tools } = await client.listTools()
    if (tools.length !== capabilities.length) throw new Error("runtime tool count differs from manifest")
    for (const capability of capabilities) {
      const tool = tools.find((entry) => entry.name === capability.toolName)
      if (!tool || !isDeepStrictEqual(tool.inputSchema, contracts[capability.toolName].inputSchema)) throw new Error(`runtime tool contract mismatch: ${capability.toolName}`)
    }
  } finally { await client.close() }
}

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
  const workbench = JSON.parse(await readFile(join(root, "vendor/infinite-canvas-web/package.json"), "utf8"))
  const pending = [
    ...Object.keys(packageJson.dependencies || {}).map((name) => ({ name, base: root })),
    ...Object.keys(workbench.dependencies || {}).map((name) => ({ name, base: join(root, "vendor/infinite-canvas-web") })),
  ]
  while (pending.length) {
    const request = pending.pop()
    if (!request) continue
    const path = join(request.base, "node_modules", ...request.name.split("/"), "package.json")
    let value
    try { value = JSON.parse(await readFile(path, "utf8")) } catch { value = packageIndex.get(request.name) }
    if (!value) continue
    const key = `${value.name || request.name}@${value.version}`
    if (packages.has(key)) continue
    packages.set(key, { name: value.name || request.name, version: value.version, license: packageLicense(value) })
    const base = dirname(path)
    pending.push(...Object.keys(value.dependencies || {}).map((name) => ({ name, base })), ...Object.keys(value.optionalDependencies || {}).map((name) => ({ name, base })))
  }
  const report = {}
  for (const value of [...packages.values()].sort((left, right) => left.name.localeCompare(right.name))) (report[value.license] ||= []).push(value)
  const allowed = new Set(["MIT", "ISC", "BSD-2-Clause", "BSD-3-Clause", "Apache-2.0", "CC-BY-4.0", "CC0-1.0", "BlueOak-1.0.0", "0BSD", "(MIT OR CC0-1.0)"])
  const unsupported = Object.keys(report).filter((license) => !allowed.has(license))
  if (unsupported.length) {
    const details = unsupported.map((license) => `${license}: ${report[license].map((value) => value.name).join(", ")}`)
    throw new Error(`unsupported production licenses:\n${details.join("\n")}`)
  }
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

function packageLicense(value) {
  if (typeof value.license === "string") return value.license
  if (value.license && typeof value.license.type === "string") return value.license.type
  const legacy = value.licenses
  if (Array.isArray(legacy) && legacy.length && legacy.every((entry) => typeof entry.type === "string")) {
    return legacy.map((entry) => entry.type).join(" OR ")
  }
  return "UNKNOWN"
}
