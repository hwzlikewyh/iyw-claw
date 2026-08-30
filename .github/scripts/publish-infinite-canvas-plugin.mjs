#!/usr/bin/env node

import { createHash } from "node:crypto"
import { execFile } from "node:child_process"
import { lstat, readFile, readdir } from "node:fs/promises"
import { promisify } from "node:util"
import { join, relative, resolve } from "node:path"

const execFileAsync = promisify(execFile)
const PLUGIN = "infinite-canvas"
const MAX_POLL_MS = 2000
const MAX_WAIT_MS = 180000

const options = parseArgs()
const source = resolve(options.source || "")
const packageValue = await inspect(source)
const version = packageValue.manifest.version
console.log(JSON.stringify({ source, slug: PLUGIN, version, files: packageValue.files.map(({ path, size, sha256 }) => ({ path, size, sha256 })), totalBytes: packageValue.totalBytes, components: packageValue.manifest.components }, null, 2))

if (options.dryRun || !options.baseUrl || !options.token) {
  if (!options.dryRun && (!options.baseUrl || !options.token)) console.log("FUSION_BASE_URL/FUSION_ADMIN_TOKEN missing; forced dry-run")
  process.exit(0)
}

const uploaded = await upload(packageValue, options)
console.log(JSON.stringify(uploaded, null, 2))

function parseArgs() {
  const result = { source: process.env.INFINITE_CANVAS_SOURCE, dryRun: false, baseUrl: (process.env.FUSION_BASE_URL || "").replace(/\/$/, ""), token: process.env.FUSION_ADMIN_TOKEN || process.env.IYW_FUSION_ADMIN_TOKEN || "", gatewayToken: process.env.FUSION_GATEWAY_TOKEN || process.env.IYW_FUSION_GATEWAY_TOKEN || "" }
  for (let index = 2; index < process.argv.length; index += 1) {
    const value = process.argv[index]
    if (value === "--dry-run") result.dryRun = true
    else if (value === "--source") result.source = process.argv[++index]
    else if (value === "--base-url") result.baseUrl = (process.argv[++index] || "").replace(/\/$/, "")
    else if (value === "--help" || value === "-h") { console.log("Usage: node .github/scripts/publish-infinite-canvas-plugin.mjs --dry-run --source <plugin-root>"); process.exit(0) }
    else throw new Error(`unknown argument: ${value}`)
  }
  if (!result.source) throw new Error("--source or INFINITE_CANVAS_SOURCE is required")
  return result
}

async function inspect(root) {
  const verify = join(root, "scripts", "verify.mjs")
  await execFileAsync(process.execPath, [verify], { cwd: root })
  const manifest = JSON.parse(await readFile(join(root, ".iyw-plugin.json"), "utf8"))
  if (manifest.name !== PLUGIN || manifest.schemaVersion !== 2 || JSON.stringify(manifest.targets) !== JSON.stringify(["iyw-claw"])) throw new Error("invalid Infinite Canvas v2 manifest")
  const paths = [".iyw-plugin.json", "runtime/dist/infinite-canvas-mcp.mjs", "widget/dist/infinite-canvas-widget.html", "contracts", "skills", "LICENSE", "THIRD_PARTY_NOTICES.md", "upstream.json", "dist/license-report.json"]
  const files = (await collect(root, paths)).sort((left, right) => left.path.localeCompare(right.path))
  return { manifest, files, totalBytes: files.reduce((sum, file) => sum + file.size, 0) }
}

async function collect(root, paths) {
  const result = []
  for (const value of paths) {
    const path = join(root, value)
    const info = await lstat(path)
    if (info.isSymbolicLink()) throw new Error(`symbolic link is not allowed: ${value}`)
    if (info.isDirectory()) result.push(...await collectDirectory(root, path))
    else result.push(await fileRecord(root, path))
  }
  return result
}

async function collectDirectory(root, directory) {
  const result = []
  for (const name of await readdir(directory)) result.push(...await collect(root, [relative(root, join(directory, name))]))
  return result
}

async function fileRecord(root, path) {
  const data = await readFile(path)
  return { path: relative(root, path).replaceAll("\\", "/"), data, size: data.length, sha256: createHash("sha256").update(data).digest("hex"), mimeType: mimeType(path) }
}

async function upload(packageValue, config) {
  const files = packageValue.files.map(({ path, size, sha256, mimeType }) => ({ path, size, sha256, mimeType }))
  const init = await api(config, "/admin/api/skills/uploads/init", { method: "POST", body: { skillId: process.env.INFINITE_CANVAS_SKILL_ID || "0", slug: PLUGIN, displayName: "Infinite Canvas", summary: "Project-persisted Infinite Canvas MCP App.", category: "design-media", iconUrl: null, tags: ["canvas", "mcp-apps", "workspace"], visibility: "public", version: packageValue.manifest.version, changelog: "Infinite Canvas widget, migration, and creative parity update.", packageType: "plugin", dependencies: [], files } })
  try {
    for (const remote of init.files) {
      const local = packageValue.files.find((file) => file.path === remote.path)
      if (!local) throw new Error(`Fusion returned unknown file ${remote.path}`)
      const signed = await api(config, "/admin/api/skills/uploads/url", { method: "POST", body: { uploadId: String(init.uploadId), fileId: String(remote.id) } })
      const response = await fetch(signed.url, { method: signed.method || "PUT", headers: signed.headers || {}, body: local.data, signal: AbortSignal.timeout(60000) })
      if (!response.ok) throw new Error(`TOS upload failed for ${local.path}: ${response.status}`)
    }
    await waitFor(async () => { const status = await api(config, "/admin/api/skills/uploads/status", { method: "POST", body: { uploadId: String(init.uploadId) } }); return status.files.every((file) => file.uploaded) }, "raw upload")
    const completed = await api(config, "/admin/api/skills/uploads/complete", { method: "POST", body: { uploadId: String(init.uploadId) } })
    const skillId = String(completed.skillId || init.skillId)
    await api(config, "/admin/api/skills/set-disabled", { method: "POST", body: { id: skillId, disabled: true } })
    const detail = await waitFor(async () => { const value = await api(config, "/admin/api/skills/detail", { method: "POST", body: { id: skillId, version: packageValue.manifest.version } }); const version = value.skill?.currentVersion; return version?.status === "ready" ? version : false }, "artifact build")
    return { skillId, version: packageValue.manifest.version, disabled: true, status: detail.status, artifact: detail.artifact || { size: detail.artifactSize, sha256: detail.artifactSha256 } }
  } catch (error) {
    await api(config, "/admin/api/skills/uploads/abort", { method: "POST", body: { uploadId: String(init.uploadId) } }).catch(() => undefined)
    throw error
  }
}

async function api(config, path, request) {
  const response = await fetch(`${config.baseUrl}${path}`, { method: request.method, headers: { "content-type": "application/json", "admin-token": config.token, "X-IYW-Admin-Actor": "codex-infinite-canvas", ...(config.gatewayToken ? { token: config.gatewayToken } : {}) }, body: JSON.stringify(request.body), signal: AbortSignal.timeout(30000) })
  const body = await response.json().catch(() => ({}))
  if (!response.ok || body.code !== 1) throw new Error(`${path}: ${body.message || response.status}`)
  return body.data
}

async function waitFor(read, label) {
  const deadline = Date.now() + MAX_WAIT_MS
  while (Date.now() < deadline) { const value = await read(); if (value) return value; await new Promise((resolvePromise) => setTimeout(resolvePromise, MAX_POLL_MS)) }
  throw new Error(`${label} timed out`)
}

function mimeType(path) { if (path.endsWith(".json")) return "application/json"; if (path.endsWith(".mjs")) return "text/javascript"; if (path.endsWith(".html")) return "text/html"; if (path.endsWith(".md")) return "text/markdown"; return "application/octet-stream" }
