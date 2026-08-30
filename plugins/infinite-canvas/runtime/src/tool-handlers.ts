import { createHash } from "node:crypto"
import { mkdir, rename, unlink, writeFile } from "node:fs/promises"
import { basename, join } from "node:path"
import { CanvasRuntimeError, invalid } from "./errors.js"
import { rejectSymlinkPath, storageRoot } from "./paths.js"
import { SceneStore } from "./scene-store.js"
import { AssetStore } from "./asset-store.js"
import { MAX_OPERATIONS, type CanvasOperation, type CanvasScene } from "./types.js"
import { listCowartPages, readCowartPage } from "./migration/cowart-reader.js"
import { mapCowartPage } from "./migration/cowart-mapper.js"
import { writeMigrationReport } from "./migration/cowart-report.js"

type JsonObject = Record<string, unknown>
export type ToolResult = { data: unknown; isError?: false }

export function createToolHandlers(sceneStore = new SceneStore(), assetStore = new AssetStore()) {
  return {
    async call(name: string, args: JsonObject): Promise<ToolResult> {
      switch (name) {
        case "render_infinite_canvas_widget": return render(sceneStore, args)
        case "get_infinite_canvas_state": return getState(sceneStore, args)
        case "get_infinite_canvas_selection": return { data: await sceneStore.readSelection(stringArg(args, "canvasId", "main")) }
        case "save_infinite_canvas_selection": return saveSelection(sceneStore, args)
        case "apply_infinite_canvas_ops": return applyOps(sceneStore, args)
        case "save_infinite_canvas_snapshot": return saveSnapshot(sceneStore, args)
        case "read_infinite_canvas_asset": return { data: await assetStore.readChunk(stringArg(args, "sha256"), integerArg(args, "offset", 0), integerArg(args, "length")) }
        case "write_infinite_canvas_asset": return writeAsset(assetStore, args)
        case "export_infinite_canvas": return exportCanvas(sceneStore, assetStore, args)
        case "migrate_cowart_canvas": return migrateCowart(sceneStore, assetStore, args)
        default: throw new CanvasRuntimeError("invalid_input", `unknown tool: ${name}`)
      }
    },
    close: () => assetStore.close(),
  }
}

async function migrateCowart(store: SceneStore, assets: AssetStore, args: JsonObject): Promise<ToolResult> {
  if (args.listOnly === true) return { data: { pages: await listCowartPages() } }
  const page = await readCowartPage(stringArg(args, "pageId"))
  const targetCanvasId = stringArg(args, "targetCanvasId", defaultMigrationCanvasId(page.pageId))
  const mapped = mapCowartPage(page, targetCanvasId)
  const dryRun = args.dryRun === true
  if (dryRun) return { data: { ...mapped, scene: sceneSummary(mapped.scene), dryRun: true } }
  if (await store.exists(targetCanvasId)) throw new CanvasRuntimeError("migration_target_exists", "migration target already exists")
  await importCowartAssets(mapped.scene, assets, mapped.warnings)
  const saved = await store.save(mapped.scene, 0)
  const reportPath = await writeMigrationReport({ schemaVersion: 1, pageId: page.pageId, targetCanvasId, dryRun: false, sourcePath: page.sourcePath, sourceSha256: page.sourceSha256, mapped: mapped.mapped, skipped: mapped.skipped, warnings: mapped.warnings, unsupportedRecords: mapped.unsupportedRecords })
  return { data: { targetCanvasId, revision: saved.revision, mapped: mapped.mapped, skipped: mapped.skipped, warnings: mapped.warnings, unsupportedRecords: mapped.unsupportedRecords, reportPath } }
}

function defaultMigrationCanvasId(pageId: string): string {
  const prefix = pageId.slice(0, 40)
  const suffix = createHash("sha256").update(pageId).digest("hex").slice(0, 16)
  return `cowart-${prefix}-${suffix}`
}

async function importCowartAssets(scene: CanvasScene, assets: AssetStore, warnings: string[]): Promise<void> {
  for (const node of scene.nodes) {
    const metadata = node.metadata ?? {}
    if (typeof metadata.sourcePath !== "string") continue
    try {
      const asset = await assets.importSource(metadata.sourcePath, basename(metadata.sourcePath), mimeForNode(node.type))
      const source = node.type === "iyw:html" ? await assets.readSourceText(metadata.sourcePath) : undefined
      node.metadata = { ...metadata, asset, ...(source === undefined ? {} : { source }) }
      delete node.metadata.sourcePath
    } catch { warnings.push(`asset_${node.id}_unavailable`) }
  }
}

function mimeForNode(type: string): string {
  if (type === "media") return "application/octet-stream"
  if (type === "iyw:html") return "text/html"
  return "application/octet-stream"
}

async function render(store: SceneStore, args: JsonObject): Promise<ToolResult> {
  const scene = await store.read(stringArg(args, "canvasId", "main"))
  return { data: { canvasId: scene.canvasId, revision: scene.revision, resourceUri: "ui://widget/infinite-canvas/canvas.html", displayMode: args.displayMode === "fullscreen" ? "fullscreen" : "inline" } }
}

async function getState(store: SceneStore, args: JsonObject): Promise<ToolResult> {
  const scene = await store.read(stringArg(args, "canvasId", "main"))
  const since = args.sinceRevision
  return since === scene.revision ? { data: { canvasId: scene.canvasId, revision: scene.revision, unchanged: true } } : { data: sceneSummary(scene) }
}

async function saveSelection(store: SceneStore, args: JsonObject): Promise<ToolResult> {
  const canvasId = stringArg(args, "canvasId")
  const revision = integerArg(args, "revision")
  const selectedNodeIds = args.selectedNodeIds
  const scene = await store.read(canvasId)
  if (scene.revision !== revision) throw new CanvasRuntimeError("revision_conflict", "selection revision changed", { latestRevision: scene.revision })
  if (!Array.isArray(selectedNodeIds) || !selectedNodeIds.every((item) => typeof item === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(item)) || selectedNodeIds.some((item) => !scene.nodes.some((node) => node.id === item))) throw invalid("selection_invalid")
  const selection = await store.saveSelection(canvasId, { revision, selectedNodeIds, updatedAt: new Date().toISOString() })
  return { data: selection }
}

async function applyOps(store: SceneStore, args: JsonObject): Promise<ToolResult> {
  const canvasId = stringArg(args, "canvasId")
  const baseRevision = integerArg(args, "baseRevision")
  if (!Array.isArray(args.operations) || args.operations.length < 1 || args.operations.length > MAX_OPERATIONS) throw invalid("operations_invalid")
  const scene = await store.apply(canvasId, baseRevision, args.operations as CanvasOperation[])
  return { data: sceneSummary(scene) }
}

async function saveSnapshot(store: SceneStore, args: JsonObject): Promise<ToolResult> {
  const canvasId = stringArg(args, "canvasId")
  const scene = args.scene as CanvasScene
  if (!scene || scene.canvasId !== canvasId) throw invalid("scene_canvas_mismatch")
  const result = await store.save(scene, integerArg(args, "baseRevision"))
  return { data: sceneSummary(result) }
}

async function writeAsset(store: AssetStore, args: JsonObject): Promise<ToolResult> {
  const sourcePath = typeof args.sourcePath === "string" ? args.sourcePath : undefined
  const uploadId = typeof args.uploadId === "string" ? args.uploadId : undefined
  if (sourcePath && uploadId) throw invalid("asset_source_chunk_conflict")
  if (sourcePath) return { data: await store.importSource(sourcePath, stringArg(args, "name", basename(sourcePath)), stringArg(args, "mimeType", "application/octet-stream"), optionalString(args, "expectedSha256")) }
  if (!uploadId) {
    const started = await store.begin(stringArg(args, "name"), stringArg(args, "mimeType"), integerArg(args, "expectedBytes"), stringArg(args, "expectedSha256"))
    if (typeof args.dataBase64 !== "string") return { data: started }
    await store.writeChunk(started.uploadId, integerArg(args, "chunkIndex", 0), args.dataBase64)
    return args.finalize === true ? { data: await store.finalize(started.uploadId) } : { data: started }
  }
  if (args.cancel === true) { await store.cancel(uploadId); return { data: { uploadId, cancelled: true } } }
  if (typeof args.dataBase64 === "string") await store.writeChunk(uploadId, integerArg(args, "chunkIndex"), args.dataBase64)
  return args.finalize === true ? { data: await store.finalize(uploadId) } : { data: { uploadId } }
}

async function exportCanvas(sceneStore: SceneStore, assets: AssetStore, args: JsonObject): Promise<ToolResult> {
  const canvasId = stringArg(args, "canvasId")
  const format = stringArg(args, "format", "json")
  if (!(["json", "html", "png", "svg"] as string[]).includes(format)) throw invalid("export_format_invalid")
  const fileName = safeFileName(stringArg(args, "fileName", `canvas-${canvasId}.${format}`))
  const exportId = optionalString(args, "exportId")
  if (exportId && !/^[A-Za-z0-9_-]{1,64}$/.test(exportId)) throw invalid("export_id_invalid")
  const exportRoot = join(storageRoot(), "exports", ...(exportId ? [exportId] : []))
  const target = join(exportRoot, fileName)
  await rejectSymlinkPath(exportRoot)
  await rejectSymlinkPath(target)
  await mkdir(exportRoot, { recursive: true })
  if (format === "json") await atomicWrite(target, JSON.stringify(await sceneStore.read(canvasId), null, 2) + "\n")
  else if (format === "html" && typeof args.sourceAssetSha256 !== "string") await atomicWrite(target, sceneToHtml(await sceneStore.read(canvasId)))
  else {
    const hash = stringArg(args, "sourceAssetSha256")
    const chunk = await assets.readChunk(hash, 0, 128 * 1024)
    if (!chunk.dataBase64) throw invalid("export_source_empty")
    const source = await readAssetFully(assets, hash, chunk)
    await atomicWrite(target, source)
  }
  return { data: { canvasId, format, relativePath: target.slice(storageRoot().length + 1).replaceAll("\\", "/") } }
}

async function readAssetFully(assets: AssetStore, hash: string, first: { dataBase64: string; bytes: number; eof: boolean }): Promise<Buffer> {
  const parts = [Buffer.from(first.dataBase64, "base64")]
  let offset = parts[0]!.length
  while (offset < first.bytes) { const next = await assets.readChunk(hash, offset, 128 * 1024); parts.push(Buffer.from(next.dataBase64, "base64")); offset += parts.at(-1)?.length ?? 0 }
  return Buffer.concat(parts)
}

function sceneSummary(scene: CanvasScene) { return { ...scene, nodes: scene.nodes.map((node) => ({ ...node, ...(node.metadata ? { metadata: safeMetadata(node.metadata) } : {}) })), connections: scene.connections } }
function safeMetadata(metadata: Record<string, unknown>): Record<string, unknown> { return Object.fromEntries(Object.entries(metadata).filter(([key, value]) => key !== "content" || typeof value !== "string" || value.length <= 10000)) }
function stringArg(args: JsonObject, key: string, fallback?: string): string { const value = args[key] ?? fallback; if (typeof value !== "string" || !value) throw invalid(`${key}_invalid`); return value }
function optionalString(args: JsonObject, key: string): string | undefined { return typeof args[key] === "string" ? args[key] : undefined }
function integerArg(args: JsonObject, key: string, fallback?: number): number { const value = args[key] ?? fallback; if (!Number.isSafeInteger(value)) throw invalid(`${key}_invalid`); return value as number }
function safeFileName(value: string): string { const name = basename(value); if (name !== value || !/^[A-Za-z0-9._-]{1,180}$/.test(name)) throw invalid("file_name_invalid"); return name }
async function atomicWrite(path: string, data: string | Buffer): Promise<void> { const temp = `${path}.tmp-${process.pid}-${Math.random().toString(36).slice(2)}`; try { await writeFile(temp, data); await rename(temp, path) } finally { await unlink(temp).catch(() => undefined) } }

function sceneToHtml(scene: CanvasScene): string {
  const nodes = scene.nodes.map((node) => {
    const metadata = node.metadata ?? {}
    const body = node.type === "iyw:html" ? `<iframe sandbox srcdoc="${escapeAttribute(sanitizeEmbeddedHtml(String(metadata.content ?? metadata.source ?? "")))}"></iframe>` : node.type === "iyw:slides" && metadata.deck && typeof metadata.deck === "object" ? slideDeckHtml(metadata.deck) : `<pre>${escapeHtml(String(metadata.text ?? metadata.content ?? node.type))}</pre>`
    return `<article class="node" style="left:${node.x}px;top:${node.y}px;width:${node.width}px;height:${node.height}px">${body}</article>`
  }).join("")
  return `<!doctype html><meta charset="utf-8"><title>${escapeHtml(scene.canvasId)}</title><style>body{margin:0;background:#101318;color:#f4f7fb;position:relative;min-width:1600px;min-height:1000px}.node{position:absolute;overflow:hidden;border:1px solid #5c6b80;background:#222a36;padding:10px;box-sizing:border-box}.node iframe{width:100%;height:100%;border:0;background:#fff}.node pre{white-space:pre-wrap}.slides-page{min-height:100%;background:#fff;color:#111;padding:12px}</style>${nodes}\n`
}

function slideDeckHtml(value: object): string {
  const pages = (value as { pages?: unknown }).pages
  if (!Array.isArray(pages)) return "<pre>Invalid slides</pre>"
  return `<div class="slides">${pages.map((page) => { const item = page && typeof page === "object" ? page as { title?: unknown; html?: unknown } : {}; return `<section class="slides-page"><h2>${escapeHtml(String(item.title ?? "Slide"))}</h2>${sanitizeEmbeddedHtml(String(item.html ?? ""))}</section>` }).join("")}</div>`
}

function sanitizeEmbeddedHtml(value: string): string {
  return value
    .replace(/<\/?(script|iframe|form|object|embed|base|link)\b[^>]*>/gi, "")
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, "")
    .replace(/\s+on[a-z-]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, "")
    .replace(/\s+(src|href|action)\s*=\s*(?:"(?!#|data:|blob:)[^"]*"|'(?!#|data:|blob:)[^']*'|(?!#|data:|blob:)[^\s>]+)/gi, "")
    .replace(/\s+style\s*=\s*(?:"[^"]*(?:url\s*\(|expression\s*\()[^"]*"|'[^']*(?:url\s*\(|expression\s*\()[^']*'|[^\s>]*(?:url\s*\(|expression\s*\()[^\s>]*)/gi, "")
}

function escapeHtml(value: string): string { return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[character] ?? character) }
function escapeAttribute(value: string): string { return escapeHtml(value).replace(/\r?\n/g, "&#10;") }
