import type { CanvasClient } from "./canvas-client.js"
import { renderAnnotationLayer } from "./plugins/annotation/annotation-layer.js"
import type { AnnotationShape } from "./plugins/annotation/types.js"
import { sanitizeHtml } from "./plugins/html/html-actions.js"
import { renderMarkdown } from "./plugins/markdown/register.js"
import { sanitizeSvg } from "./plugins/svg/register.js"
import { validateDeck, type SlideDeck } from "./plugins/slides/types.js"

export type NodeData = { id: string; type: string; x: number; y: number; width: number; height: number; metadata?: Record<string, unknown>; [key: string]: unknown }
export type Scene = { canvasId: string; revision: number; nodes: NodeData[]; connections: Array<{ id: string; fromNodeId: string; toNodeId: string }>; backgroundMode: string; showImageInfo: boolean; viewport: { x: number; y: number; k: number } }

type RenderOptions = {
  client: CanvasClient
  plugins: ReadonlyMap<string, unknown>
  selected: ReadonlySet<string>
  onAnnotationChange: (nodeId: string, shapes: AnnotationShape[]) => void
}

export function renderScene(sceneElement: HTMLElement, scene: Scene, options: RenderOptions): void {
  options.client.retainAssets(scene.nodes.flatMap((node) => isAsset(node.metadata?.asset) ? [node.metadata.asset] : []))
  sceneElement.style.transform = `translate(${scene.viewport.x}px, ${scene.viewport.y}px) scale(${scene.viewport.k})`
  const nodes = scene.nodes.map((node) => renderNode(node, options))
  sceneElement.replaceChildren(renderConnections(scene), ...nodes)
}

function renderNode(node: NodeData, options: RenderOptions): HTMLElement {
  const element = document.createElement("div")
  element.className = `node${options.selected.has(node.id) ? " selected" : ""}`
  element.dataset.nodeId = node.id
  element.style.left = `${node.x}px`
  element.style.top = `${node.y}px`
  element.style.width = `${node.width}px`
  element.style.height = `${node.height}px`
  renderNodeContent(element, node, options)
  return element
}

function renderNodeContent(element: HTMLElement, node: NodeData, options: RenderOptions): void {
  const metadata = node.metadata ?? {}
  if (node.type.startsWith("iyw:") && !options.plugins.has(node.type)) return showText(element, `Unsupported plugin node: ${node.type}`)
  if (node.type === "media" && isAsset(metadata.asset)) {
    showText(element, "Loading asset...")
    void renderMediaAsset(element, metadata.asset, options.client).catch(() => { if (element.isConnected) showText(element, "Asset unavailable") })
    return
  }
  if (node.type === "iyw:annotation-layer" && Array.isArray(metadata.shapes)) {
    renderAnnotationLayer(element, metadata.shapes as AnnotationShape[], (shapes) => options.onAnnotationChange(node.id, shapes))
    return
  }
  if (node.type === "iyw:html") return renderHtml(element, metadata)
  if (node.type === "iyw:markdown" || node.type === "markdown:doc") return renderMarkdownNode(element, metadata)
  if (node.type === "iyw:svg" || node.type === "svg:vector") return renderSvgNode(element, metadata)
  if (node.type === "iyw:slides" && metadata.deck && typeof metadata.deck === "object") return renderSlidesNode(element, metadata.deck as SlideDeck)
  if (node.type === "creative-request") return renderCreativeRequest(element, metadata)
  showText(element, typeof metadata.text === "string" ? metadata.text : node.type)
}

function renderCreativeRequest(element: HTMLElement, metadata: Record<string, unknown>): void {
  const status = typeof metadata.status === "string" ? metadata.status : "pending"
  const prompt = typeof metadata.prompt === "string" ? metadata.prompt : "Creative request"
  const error = typeof metadata.errorCode === "string" ? ` (${metadata.errorCode})` : ""
  showText(element, `${status}: ${prompt}${error}`)
}

function renderHtml(element: HTMLElement, metadata: Record<string, unknown>): void {
  const frame = document.createElement("iframe")
  frame.setAttribute("sandbox", "")
  frame.srcdoc = sanitizeHtml(typeof metadata.html === "string" ? metadata.html : typeof metadata.source === "string" ? metadata.source : typeof metadata.content === "string" ? metadata.content : "<p>HTML draft</p>")
  frame.style.cssText = "width:100%;height:100%;border:0;background:#fff"
  element.replaceChildren(frame)
}

function renderMarkdownNode(element: HTMLElement, metadata: Record<string, unknown>): void {
  const preview = document.createElement("div")
  preview.innerHTML = renderMarkdown(typeof metadata.content === "string" ? metadata.content : "") || "<p>Markdown</p>"
  preview.style.cssText = "height:100%;overflow:auto;background:#fff;color:#17212b;padding:10px"
  element.replaceChildren(preview)
}

function renderSvgNode(element: HTMLElement, metadata: Record<string, unknown>): void {
  const preview = document.createElement("div")
  preview.innerHTML = sanitizeSvg(typeof metadata.content === "string" ? metadata.content : "")
  preview.style.cssText = "height:100%;width:100%;display:grid;place-items:center;background:transparent"
  element.replaceChildren(preview)
}

function renderSlidesNode(element: HTMLElement, deck: SlideDeck): void {
  try {
    validateDeck(deck)
    showText(element, deck.pages.find((page) => page.id === deck.activePageId)?.title || deck.title)
  } catch { showText(element, "Invalid slide deck") }
}

async function renderMediaAsset(element: HTMLElement, asset: Asset, client: CanvasClient): Promise<void> {
  const url = await client.objectUrl(asset.sha256, asset.bytes, asset.mimeType)
  if (!element.isConnected) return
  const media = asset.mimeType.startsWith("image/") ? document.createElement("img") : asset.mimeType.startsWith("video/") ? document.createElement("video") : document.createElement("audio")
  media.src = url
  if (media instanceof HTMLMediaElement) media.controls = true
  media.style.cssText = "width:100%;height:100%;object-fit:contain"
  element.replaceChildren(media)
}

function renderConnections(scene: Scene): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg")
  svg.setAttribute("viewBox", "0 0 1600 1000")
  svg.style.cssText = "position:absolute;inset:0;width:1600px;height:1000px;pointer-events:none;overflow:visible"
  const positions = new Map(scene.nodes.map((node) => [node.id, node]))
  for (const connection of scene.connections ?? []) {
    const from = positions.get(connection.fromNodeId)
    const to = positions.get(connection.toNodeId)
    if (!from || !to) continue
    const line = document.createElementNS(svg.namespaceURI, "line")
    line.setAttribute("x1", `${from.x + from.width / 2}`)
    line.setAttribute("y1", `${from.y + from.height / 2}`)
    line.setAttribute("x2", `${to.x + to.width / 2}`)
    line.setAttribute("y2", `${to.y + to.height / 2}`)
    line.setAttribute("stroke", "#72b7ff")
    line.setAttribute("stroke-width", "2")
    svg.append(line)
  }
  return svg
}

type Asset = { sha256: string; bytes: number; mimeType: string }
function isAsset(value: unknown): value is Asset { return Boolean(value && typeof value === "object" && typeof (value as Record<string, unknown>).sha256 === "string" && typeof (value as Record<string, unknown>).bytes === "number" && typeof (value as Record<string, unknown>).mimeType === "string") }
function showText(element: HTMLElement, value: string): void { element.textContent = value }
