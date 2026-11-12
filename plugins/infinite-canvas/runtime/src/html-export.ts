import type { AssetStore } from "./asset-store.js"
import type { CanvasNodeData, CanvasScene } from "./types.js"
import { invalid } from "./errors.js"

const CHUNK_BYTES = 128 * 1024
const IMAGE_MIMES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"])

export async function readAssetFully(assets: AssetStore, hash: string): Promise<Buffer> {
  const parts: Buffer[] = []
  let offset = 0, total = 0
  do {
    const chunk = await assets.readChunk(hash, offset, CHUNK_BYTES)
    const bytes = Buffer.from(chunk.dataBase64, "base64")
    if (!offset) total = chunk.bytes
    if (chunk.bytes !== total || !bytes.length || offset + bytes.length > total) throw invalid("export_source_changed")
    parts.push(bytes); offset += bytes.length
  } while (offset < total)
  return Buffer.concat(parts)
}

export async function sceneToHtml(scene: CanvasScene, assets: AssetStore): Promise<string> {
  const nodes: string[] = []
  for (const node of scene.nodes) {
    const body = await nodeBody(node, assets)
    nodes.push(`<article class="node" style="left:${node.x}px;top:${node.y}px;width:${node.width}px;height:${node.height}px">${body}</article>`)
  }
  const left = Math.min(0, ...scene.nodes.map((node) => node.x))
  const top = Math.min(0, ...scene.nodes.map((node) => node.y))
  const width = Math.max(1600, ...scene.nodes.map((node) => node.x + node.width)) - left
  const height = Math.max(1000, ...scene.nodes.map((node) => node.y + node.height)) - top
  const stage = `<main style="position:relative;width:${width}px;height:${height}px"><div style="transform:translate(${-left}px,${-top}px)">${connectionSvg(scene)}${nodes.join("")}</div></main>`
  return `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; frame-src about:"><title>${escapeHtml(scene.canvasId)}</title><style>body{margin:0;background:#101318;color:#f4f7fb}.node{position:absolute;overflow:hidden;border:1px solid #5c6b80;background:#222a36;padding:10px;box-sizing:border-box}.node iframe{width:100%;height:100%;border:0;background:#fff}.node img{width:100%;height:100%;object-fit:contain}.node pre{white-space:pre-wrap}.slides-page{min-height:100%;background:#fff;color:#111;padding:12px}</style>${stage}\n`
}

async function nodeBody(node: CanvasNodeData, assets: AssetStore): Promise<string> {
  const metadata = node.metadata ?? {}
  if (node.type === "media" && metadata.asset && typeof metadata.asset === "object") {
    const asset = metadata.asset as Record<string, unknown>
    if (typeof asset.sha256 === "string" && typeof asset.mimeType === "string" && IMAGE_MIMES.has(asset.mimeType)) {
      const bytes = await readAssetFully(assets, asset.sha256)
      return `<img alt="${escapeHtml(node.id)}" src="data:${asset.mimeType};base64,${bytes.toString("base64")}">`
    }
  }
  if (node.type === "iyw:html") return `<iframe sandbox srcdoc="${escapeAttribute(sanitizeEmbeddedHtml(String(metadata.content ?? metadata.source ?? "")))}"></iframe>`
  if (node.type === "iyw:slides" && metadata.deck && typeof metadata.deck === "object") return slideDeckHtml(metadata.deck)
  return `<pre>${escapeHtml(String(metadata.text ?? metadata.content ?? node.type))}</pre>`
}

function connectionSvg(scene: CanvasScene): string {
  const positions = new Map(scene.nodes.map((node) => [node.id, node]))
  const lines = scene.connections.flatMap((connection) => {
    const from = positions.get(connection.fromNodeId), to = positions.get(connection.toNodeId)
    return from && to ? [`<line x1="${from.x + from.width / 2}" y1="${from.y + from.height / 2}" x2="${to.x + to.width / 2}" y2="${to.y + to.height / 2}" stroke="#72b7ff" stroke-width="2"/>`] : []
  })
  return `<svg xmlns="http://www.w3.org/2000/svg" style="position:absolute;width:1600px;height:1000px;overflow:visible;pointer-events:none">${lines.join("")}</svg>`
}

function slideDeckHtml(value: object): string {
  const pages = (value as { pages?: unknown }).pages
  if (!Array.isArray(pages)) return "<pre>Invalid slides</pre>"
  return `<div class="slides">${pages.map((page) => { const item = page && typeof page === "object" ? page as { title?: unknown; html?: unknown } : {}; return `<section class="slides-page"><h2>${escapeHtml(String(item.title ?? "Slide"))}</h2>${sanitizeEmbeddedHtml(String(item.html ?? ""))}</section>` }).join("")}</div>`
}

function sanitizeEmbeddedHtml(value: string): string {
  return value
    .replace(/<(script|iframe|form|object|embed|style)\b[^>]*>[\s\S]*?<\/\1\s*>/gi, "")
    .replace(/<\/?(script|iframe|form|object|embed|base|link)\b[^>]*>/gi, "")
    .replace(/\s+on[a-z-]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)/gi, "")
    .replace(/\s+(src|href|action)\s*=\s*(?:"(?!#|data:)[^"]*"|'(?!#|data:)[^']*'|(?!#|data:)[^\s>]+)/gi, "")
    .replace(/\s+style\s*=\s*(?:"[^"]*(?:url\s*\(|expression\s*\()[^"]*"|'[^']*(?:url\s*\(|expression\s*\()[^']*'|[^\s>]*(?:url\s*\(|expression\s*\()[^\s>]*)/gi, "")
}

function escapeHtml(value: string): string { return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character) }
function escapeAttribute(value: string): string { return escapeHtml(value).replace(/\r?\n/g, "&#10;") }
