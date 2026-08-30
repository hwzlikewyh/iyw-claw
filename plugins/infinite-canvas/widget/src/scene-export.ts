import type { CanvasClient } from "./canvas-client.js"

type ExportNode = { id: string; type: string; x: number; y: number; width: number; height: number; metadata?: Record<string, unknown> }
type ExportScene = { nodes: ExportNode[] }
type PersistExport = (sha256: string, format: "png" | "svg") => Promise<string>

const MAX_PIXELS = 16_000_000

export async function exportRenderedScene(client: CanvasClient, scene: ExportScene, format: "png" | "svg", persist: PersistExport): Promise<string> {
  const svg = await sceneSvg(client, scene)
  const blob = format === "svg" ? new Blob([svg], { type: "image/svg+xml" }) : await rasterize(svg)
  const uploaded = await client.upload(blob, `canvas-export.${format}`, format === "svg" ? "image/svg+xml" : "image/png")
  if (typeof uploaded.sha256 !== "string") throw new Error("canvas export upload failed")
  return persist(uploaded.sha256, format)
}

async function sceneSvg(client: CanvasClient, scene: ExportScene): Promise<string> {
  const width = Math.max(1600, ...scene.nodes.map((node) => Math.ceil(node.x + node.width)))
  const height = Math.max(1000, ...scene.nodes.map((node) => Math.ceil(node.y + node.height)))
  if (width * height > MAX_PIXELS) throw new Error("canvas export exceeds 16 megapixel limit")
  const content: string[] = [`<rect width="${width}" height="${height}" fill="#101318"/>`]
  for (const node of scene.nodes) content.push(await nodeSvg(client, node))
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${content.join("")}</svg>`
}

async function nodeSvg(client: CanvasClient, node: ExportNode): Promise<string> {
  const metadata = node.metadata ?? {}
  const frame = `<rect x="${node.x}" y="${node.y}" width="${node.width}" height="${node.height}" rx="4" fill="#222a36" stroke="#5c6b80"/>`
  if (node.type === "media" && isAsset(metadata.asset)) {
    if (metadata.asset.mimeType.startsWith("image/")) {
      const source = await assetDataUri(client, metadata.asset)
      return `${frame}<image x="${node.x}" y="${node.y}" width="${node.width}" height="${node.height}" preserveAspectRatio="xMidYMid meet" href="${source}"/>`
    }
    return `${frame}${label(node.x + 12, node.y + 28, `${node.type}: ${metadata.asset.mimeType}`)}`
  }
  if (node.type === "iyw:annotation-layer" && Array.isArray(metadata.shapes)) return `${frame}<g transform="translate(${node.x} ${node.y}) scale(${node.width} ${node.height})">${annotationMarkup(metadata.shapes)}</g>`
  const value = node.type === "iyw:slides" && metadata.deck && typeof metadata.deck === "object" ? String((metadata.deck as { title?: string }).title ?? "Slides") : typeof metadata.text === "string" ? metadata.text : typeof metadata.content === "string" ? metadata.content.replace(/<[^>]+>/g, " ").trim() : node.type
  return `${frame}${label(node.x + 12, node.y + 28, value.slice(0, 240))}`
}

function annotationMarkup(shapes: unknown[]): string {
  return shapes.map((raw) => {
    if (!raw || typeof raw !== "object") return ""
    const shape = raw as Record<string, unknown>
    const color = typeof shape.color === "string" && /^#[0-9a-f]{6}$/i.test(shape.color) ? shape.color : "#ffb454"
    if (shape.type === "arrow" && point(shape.from) && point(shape.to)) return `<line x1="${point(shape.from)!.x}" y1="${point(shape.from)!.y}" x2="${point(shape.to)!.x}" y2="${point(shape.to)!.y}" stroke="${color}" stroke-width="0.006"/>`
    if ((shape.type === "rect" || shape.type === "ellipse") && numbers(shape, ["x", "y", "width", "height"])) return shape.type === "rect" ? `<rect x="${shape.x}" y="${shape.y}" width="${shape.width}" height="${shape.height}" fill="none" stroke="${color}" stroke-width="0.006"/>` : `<ellipse cx="${Number(shape.x) + Number(shape.width) / 2}" cy="${Number(shape.y) + Number(shape.height) / 2}" rx="${Math.abs(Number(shape.width)) / 2}" ry="${Math.abs(Number(shape.height)) / 2}" fill="none" stroke="${color}" stroke-width="0.006"/>`
    if (shape.type === "text" && numbers(shape, ["x", "y"]) && typeof shape.text === "string") return `<text x="${shape.x}" y="${shape.y}" fill="${color}" font-size="0.04">${escapeXml(shape.text)}</text>`
    if (shape.type === "freehand" && Array.isArray(shape.points)) return `<polyline points="${shape.points.map(point).filter(Boolean).map((item) => `${item!.x},${item!.y}`).join(" ")}" fill="none" stroke="${color}" stroke-width="${Number(shape.width) || 0.006}"/>`
    return ""
  }).join("")
}

function isAsset(value: unknown): value is { sha256: string; bytes: number; mimeType: string } { return Boolean(value && typeof value === "object" && typeof (value as Record<string, unknown>).sha256 === "string" && typeof (value as Record<string, unknown>).bytes === "number" && typeof (value as Record<string, unknown>).mimeType === "string") }
function point(value: unknown): { x: number; y: number } | undefined { if (!value || typeof value !== "object") return undefined; const item = value as Record<string, unknown>; return typeof item.x === "number" && typeof item.y === "number" ? { x: item.x, y: item.y } : undefined }
function numbers(value: Record<string, unknown>, keys: string[]): boolean { return keys.every((key) => typeof value[key] === "number" && Number.isFinite(value[key])) }
function label(x: number, y: number, value: string): string { return `<text x="${x}" y="${y}" fill="#f4f7fb" font-family="sans-serif" font-size="16">${escapeXml(value)}</text>` }
function escapeXml(value: string): string { return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[character] ?? character) }

async function assetDataUri(client: CanvasClient, asset: { sha256: string; bytes: number; mimeType: string }): Promise<string> {
  const url = await client.objectUrl(asset.sha256, asset.bytes, asset.mimeType)
  const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer())
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += 0x8000) binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000))
  return `data:${asset.mimeType};base64,${btoa(binary)}`
}

async function rasterize(svg: string): Promise<Blob> {
  const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }))
  try {
    const image = await loadImage(url)
    const canvas = document.createElement("canvas")
    canvas.width = image.naturalWidth
    canvas.height = image.naturalHeight
    const context = canvas.getContext("2d")
    if (!context) throw new Error("canvas 2d context is unavailable")
    context.drawImage(image, 0, 0)
    return await new Promise((resolve, reject) => canvas.toBlob((value) => value ? resolve(value) : reject(new Error("canvas PNG export failed")), "image/png"))
  } finally { URL.revokeObjectURL(url) }
}

function loadImage(url: string): Promise<HTMLImageElement> { return new Promise((resolve, reject) => { const image = new Image(); image.onload = () => resolve(image); image.onerror = () => reject(new Error("canvas SVG could not be rasterized")); image.src = url }) }
