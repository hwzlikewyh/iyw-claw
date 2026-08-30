import { createCanvasApp } from "./mcp-app.js"
import { CanvasClient } from "./canvas-client.js"
import { createSceneClient } from "./scene-client.js"
import { OperationBatcher } from "./operation-batcher.js"
import { CreativeFlow } from "./creative-flow.js"
import { newCreativeRequest } from "./creative-request.js"
import { addMigrationAction } from "./widget-toolbar.js"
import type { AnnotationShape } from "./plugins/annotation/types.js"
import { validateDeck, type SlideDeck } from "./plugins/slides/types.js"
import { openSlidePresenter } from "./presenter-flow.js"
import { flattenAnnotations } from "./plugins/annotation/annotation-flatten.js"
import { registerBuiltinPlugins } from "./plugins/register-builtins.js"
import { renderMigrationResult } from "./migration/cowart-migration-result.js"
import { exportRenderedScene } from "./scene-export.js"
import { renderScene, type NodeData, type Scene } from "./scene-renderer.js"
import { installWidgetStyle } from "./widget-style.js"
import { callCanvasTool } from "./host-call.js"
const app = createCanvasApp()
const pluginRegistry = registerBuiltinPlugins()
const root = document.querySelector<HTMLElement>("#app")
if (!root) throw new Error("widget root is missing")
installWidgetStyle()
let canvasId = "main"
let sceneLoaded = false
let sceneEpoch = 0
let localMutationEpoch = 0
let scene: Scene = { canvasId, revision: 0, nodes: [], connections: [], backgroundMode: "dots", showImageInfo: true, viewport: { x: 0, y: 0, k: 1 } }
let selected = new Set<string>()
let presenterCleanup: (() => void) | undefined
const shell = document.createElement("section")
shell.className = "shell"
shell.innerHTML = `<div class="toolbar"><span class="title">Infinite Canvas</span><button data-action="add">Add text</button><button data-action="html-node">HTML</button><button data-action="markdown-node">Markdown</button><button data-action="svg-node">SVG</button><button data-action="slides-node">Slides</button><button data-action="edit">Edit selected</button><button data-action="retry">Retry failed</button><button data-action="annotate">Annotate</button><button data-action="present">Present</button><button data-action="export">Export HTML</button><button data-action="export-slides">Export deck</button><button data-action="export-svg">Export SVG</button><button data-action="export-png">Export PNG</button><button data-action="image">Generate image</button><button data-action="html">Web draft</button><button data-action="slides">Slides</button><button data-action="fullscreen">Fullscreen</button><button data-action="refresh">Refresh</button><button data-action="retry-connect">Retry connection</button></div><div class="surface"><div class="scene"></div></div><div class="status">Connecting to iyw-claw...</div>`
root.replaceChildren(shell)
const sceneElement = shell.querySelector<HTMLElement>(".scene")!
const surfaceElement = shell.querySelector<HTMLElement>(".surface")!
const statusElement = shell.querySelector<HTMLElement>(".status")!
const client = new CanvasClient((name, args) => callCanvasTool(app, name, args), () => canvasId)
const sceneClient = createSceneClient(client)
const gestureBatcher = new OperationBatcher()
addMigrationAction(shell.querySelector<HTMLElement>(".toolbar")!, app, (value) => {
  shell.querySelector(".migration-result")?.remove()
  const result = renderMigrationResult(value as { pageId: string; targetCanvasId: string; mapped: number; skipped: number; warnings: string[]; reportPath?: string })
  result.className = "migration-result"
  shell.querySelector(".surface")?.prepend(result)
  statusElement.textContent = "Cowart migration completed"
})
function render() {
  renderScene(sceneElement, scene, { client, plugins: pluginRegistry, selected, onAnnotationChange: (nodeId, shapes) => { void updateAnnotationShapes(nodeId, shapes).catch(showError) } })
  statusElement.textContent = `${scene.canvasId} · revision ${scene.revision} · ${scene.nodes.length} nodes`
  statusElement.classList.remove("error")
}
const creative = new CreativeFlow({ app, client, assets: client.assetClient(), getScene: () => scene, getSelected: () => selected, setScene: (next) => { scene = next }, render, report: showError })
type DragState = { nodeId: string; pointerId: number; startX: number; startY: number; originX: number; originY: number }
let dragState: DragState | undefined
type PanState = { pointerId: number; startX: number; startY: number; originX: number; originY: number }
let panState: PanState | undefined
sceneElement.addEventListener("pointerdown", (event) => {
  if ((event.target as HTMLElement).closest("[data-annotation-editor]")) return
  const target = (event.target as HTMLElement).closest<HTMLElement>("[data-node-id]")
  const node = target ? scene.nodes.find((item) => item.id === target.dataset.nodeId) : undefined
  if (!target || !node) return
  selected = new Set([node.id])
  localMutationEpoch += 1
  dragState = { nodeId: node.id, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, originX: node.x, originY: node.y }
  render()
  sceneElement.querySelector<HTMLElement>(`[data-node-id="${node.id}"]`)?.setPointerCapture(event.pointerId)
})
sceneElement.addEventListener("pointermove", (event) => {
  if (!dragState || dragState.pointerId !== event.pointerId) return
  const node = scene.nodes.find((item) => item.id === dragState?.nodeId)
  if (!node) return
  node.x = dragState.originX + (event.clientX - dragState.startX) / scene.viewport.k
  node.y = dragState.originY + (event.clientY - dragState.startY) / scene.viewport.k
  const element = sceneElement.querySelector<HTMLElement>(`[data-node-id="${node.id}"]`)
  if (element) { element.style.left = `${node.x}px`; element.style.top = `${node.y}px` }
})
sceneElement.addEventListener("pointerup", (event) => { if (dragState?.pointerId === event.pointerId) void finishDrag().catch(showError) })
sceneElement.addEventListener("pointercancel", (event) => { if (dragState?.pointerId === event.pointerId) dragState = undefined })
async function finishDrag(): Promise<void> {
  const current = dragState
  dragState = undefined
  if (!current) return
  const node = scene.nodes.find((item) => item.id === current.nodeId)
  if (!node || (node.x === current.originX && node.y === current.originY)) return
  gestureBatcher.add({ type: "update_node", nodeId: node.id, patch: { x: node.x, y: node.y } })
  try { await gestureBatcher.flush(async (operations) => { const next = await sceneClient.applyOps(operations, scene.revision); scene = { ...scene, ...(next as unknown as Scene) }; render() }) }
  catch (error) { node.x = current.originX; node.y = current.originY; render(); throw error }
}
surfaceElement.addEventListener("wheel", (event) => {
  if ((event.target as HTMLElement).closest(".node")) return
  event.preventDefault()
  const nextK = Math.max(0.05, Math.min(5, scene.viewport.k * (event.deltaY > 0 ? 0.9 : 1.1)))
  if (nextK === scene.viewport.k) return
  void client.apply([{ type: "set_viewport", viewport: { ...scene.viewport, k: nextK } }], scene.revision).then((next) => { scene = { ...scene, ...(next as unknown as Scene) }; render() }).catch(showError)
}, { passive: false })
surfaceElement.addEventListener("pointerdown", (event) => {
  if ((event.target as HTMLElement).closest(".node")) return
  panState = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, originX: scene.viewport.x, originY: scene.viewport.y }
  localMutationEpoch += 1
  surfaceElement.setPointerCapture(event.pointerId)
  event.preventDefault()
})
surfaceElement.addEventListener("pointermove", (event) => {
  if (!panState || panState.pointerId !== event.pointerId) return
  scene.viewport.x = panState.originX + event.clientX - panState.startX
  scene.viewport.y = panState.originY + event.clientY - panState.startY
  sceneElement.style.transform = `translate(${scene.viewport.x}px, ${scene.viewport.y}px) scale(${scene.viewport.k})`
})
surfaceElement.addEventListener("pointerup", (event) => { if (panState?.pointerId === event.pointerId) void finishPan().catch(showError) })
surfaceElement.addEventListener("pointercancel", (event) => { if (panState?.pointerId === event.pointerId) panState = undefined })
async function finishPan(): Promise<void> {
  const current = panState
  panState = undefined
  if (!current) return
  if (surfaceElement.hasPointerCapture(current.pointerId)) surfaceElement.releasePointerCapture(current.pointerId)
  try {
    const next = await client.apply([{ type: "set_viewport", viewport: scene.viewport }], scene.revision)
    scene = { ...scene, ...(next as unknown as Scene) }
    render()
  } catch (error) {
    scene.viewport = { x: current.originX, y: current.originY, k: scene.viewport.k }
    render()
    throw error
  }
}
function showError(error: unknown) {
  statusElement.textContent = error instanceof Error ? error.message : "Canvas runtime error"
  statusElement.classList.add("error")
}
async function refresh() {
  if (client.hasPendingWrites()) return
  const epoch = sceneEpoch
  const mutationEpoch = localMutationEpoch
  const requestedCanvasId = canvasId
  const requestedRevision = scene.revision
  const value = await sceneClient.readScene(sceneLoaded ? scene.revision : undefined)
  if (!value) return
  if (epoch !== sceneEpoch || requestedCanvasId !== canvasId || mutationEpoch !== localMutationEpoch || value.revision < requestedRevision) return
  const viewport = scene.viewport
  const previousSelection = selected
  scene = value as Scene
  sceneLoaded = true
  scene.viewport = viewport
  selected = new Set([...previousSelection].filter((id) => scene.nodes.some((node) => node.id === id)))
  render()
}
async function addText() {
  const node: NodeData = { id: `text-${crypto.randomUUID().slice(0, 8)}`, type: "text", x: 120 + scene.nodes.length * 24, y: 120 + scene.nodes.length * 24, width: 240, height: 80, metadata: { text: "New canvas note" } }
  localMutationEpoch += 1
  const value = await client.apply([{ type: "add_node", node }], scene.revision)
  scene = { ...scene, ...(value as unknown as Scene) }
  render()
}
async function addContentNode(type: "iyw:html" | "markdown:doc" | "svg:vector" | "iyw:slides"): Promise<void> {
  const index = scene.nodes.length
  const metadata = type === "iyw:html" ? { content: "<h2>New HTML draft</h2><p>Edit this node to continue.</p>" } : type === "markdown:doc" ? { content: "# New Markdown\n\nEdit this node to continue." } : type === "svg:vector" ? { content: "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 200 120\"><rect width=\"200\" height=\"120\" fill=\"#24364b\"/><text x=\"20\" y=\"65\" fill=\"#fff\">New SVG</text></svg>" } : { deck: { schemaVersion: 1 as const, title: "New Slides", theme: { background: "#101318", foreground: "#f4f7fb", accent: "#72b7ff" }, pages: [{ id: "page-1", title: "Page 1", html: "<h1>New Slides</h1>", notes: "" }], activePageId: "page-1" } }
  const node: NodeData = { id: `${type.replace(/[^a-z]+/g, "-")}-${crypto.randomUUID().slice(0, 8)}`, type, x: 120 + (index % 4) * 280, y: 120 + Math.floor(index / 4) * 220, width: type === "svg:vector" ? 260 : 300, height: type === "iyw:slides" ? 190 : 150, metadata }
  localMutationEpoch += 1
  const next = await client.apply([{ type: "add_node", node }], scene.revision)
  scene = { ...scene, ...(next as unknown as Scene) }
  render()
}
async function exportHtml(): Promise<void> {
  const value = await callCanvasTool(app, "export_infinite_canvas", { canvasId, format: "html", fileName: `canvas-${canvasId}.html` })
  statusElement.textContent = typeof value.relativePath === "string" ? `Exported ${value.relativePath}` : "Export complete"
}
async function exportRendered(format: "png" | "svg"): Promise<void> {
  const relativePath = await exportRenderedScene(client, scene, format, async (sha256, requestedFormat) => {
    const value = await callCanvasTool(app, "export_infinite_canvas", { canvasId, format: requestedFormat, sourceAssetSha256: sha256, fileName: `canvas-${canvasId}.${requestedFormat}` })
    return typeof value.relativePath === "string" ? value.relativePath : `canvas-${canvasId}.${requestedFormat}`
  })
  statusElement.textContent = `Exported ${relativePath}`
}
async function editSelected(): Promise<void> {
  const node = scene.nodes.find((item) => selected.has(item.id))
  if (node?.type === "iyw:slides" && node.metadata?.deck && typeof node.metadata.deck === "object") return editSlides(node)
  if (!node || !["iyw:html", "html:render", "iyw:markdown", "markdown:doc", "iyw:svg", "svg:vector", "text"].includes(node.type)) return
  const current = typeof node.metadata?.content === "string" ? node.metadata.content : typeof node.metadata?.text === "string" ? node.metadata.text : typeof node.metadata?.source === "string" ? node.metadata.source : ""
  const value = window.prompt("Content", current)
  if (value === null) return
  const metadata = { ...(node.metadata ?? {}), ...(node.type.includes("markdown") ? { content: value } : node.type.includes("svg") ? { content: value } : node.type === "text" ? { text: value } : { content: value, source: value }) }
  localMutationEpoch += 1
  const next = await client.apply([{ type: "update_node", nodeId: node.id, patch: { metadata } }], scene.revision)
  scene = { ...scene, ...(next as unknown as Scene) }
  render()
}
async function editSlides(node: NodeData): Promise<void> {
  const deck = node.metadata?.deck as SlideDeck
  validateDeck(deck)
  const page = deck.pages.find((item) => item.id === deck.activePageId)
  if (!page) return
  const html = window.prompt("Slide HTML", page.html)
  if (html === null) return
  const nextDeck: SlideDeck = { ...deck, pages: deck.pages.map((item) => item.id === page.id ? { ...item, html } : item) }
  localMutationEpoch += 1
  const next = await client.apply([{ type: "update_node", nodeId: node.id, patch: { metadata: { ...(node.metadata ?? {}), deck: nextDeck } } }], scene.revision)
  scene = { ...scene, ...(next as unknown as Scene) }
  render()
}
async function addAnnotation(): Promise<void> {
  const node = scene.nodes.find((item) => selected.has(item.id))
  if (!node) return
  const shapes: AnnotationShape[] = [{ id: `rect-${crypto.randomUUID().slice(0, 8)}`, type: "rect", x: 0.08, y: 0.08, width: 0.84, height: 0.84, color: "#ffb454" }]
  const annotation: NodeData = { id: `annotation-${crypto.randomUUID().slice(0, 8)}`, type: "iyw:annotation-layer", x: node.x, y: node.y, width: node.width, height: node.height, metadata: { imageNodeId: node.id, shapes } }
  localMutationEpoch += 1
  const next = await client.apply([{ type: "add_node", node: annotation }], scene.revision)
  scene = { ...scene, ...(next as unknown as Scene) }
  render()
  const request = newCreativeRequest("image.annotation-edit", canvasId, "Apply the annotations to the selected image", [node.id], node.id, scene.revision)
  const pending: NodeData = { id: `request-${request.requestId.slice(0, 8)}`, type: "creative-request", x: node.x + node.width + 32, y: node.y, width: 300, height: 72, metadata: { requestId: request.requestId, action: request.action, prompt: request.prompt, status: "pending", selectionNodeIds: [node.id], targetNodeId: node.id } }
  localMutationEpoch += 1
  const pendingScene = await client.apply([{ type: "add_node", node: pending }], scene.revision)
  scene = { ...scene, ...(pendingScene as unknown as Scene) }
  render()
  const asset = node.metadata?.asset
  if (!asset || typeof asset !== "object") { await creative.markNodeFailed(pending.id, "annotation_asset_missing"); return }
  const reference = asset as { sha256?: string; bytes?: number; mimeType?: string }
  try {
    if (typeof reference.sha256 !== "string" || typeof reference.bytes !== "number" || typeof reference.mimeType !== "string" || !reference.mimeType.startsWith("image/")) throw new Error("annotation asset is invalid")
    const sourceUrl = await client.objectUrl(reference.sha256, reference.bytes, reference.mimeType)
    const flattened = await flattenAnnotations(sourceUrl, shapes, reference.bytes)
    const uploaded = await client.upload(flattened, `annotation-${node.id}.png`, "image/png") as { sha256?: string }
    if (typeof uploaded.sha256 !== "string") throw new Error("annotation asset upload failed")
    await creative.send({ ...request, inputAssetSha256: uploaded.sha256 })
  } catch (error) {
    await creative.markNodeFailed(pending.id, error)
    throw error
  }
}
async function updateAnnotationShapes(nodeId: string, shapes: AnnotationShape[]): Promise<void> {
  const node = scene.nodes.find((item) => item.id === nodeId)
  if (!node) return
  localMutationEpoch += 1
  const next = await client.apply([{ type: "update_node", nodeId, patch: { metadata: { ...(node.metadata ?? {}), shapes } } }], scene.revision)
  scene = { ...scene, ...(next as unknown as Scene) }
  render()
}
async function presentSelected(): Promise<void> {
  const node = scene.nodes.find((item) => selected.has(item.id))
  const deck = node?.metadata?.deck
  if (!node || node.type !== "iyw:slides" || !deck || typeof deck !== "object") return
  presenterCleanup?.()
  presenterCleanup = await openSlidePresenter(app, deck as SlideDeck, () => { presenterCleanup = undefined })
}
shell.addEventListener("click", (event) => {
  const target = event.target as HTMLElement
  if (target.closest("[data-annotation-editor]")) return
  const action = target.closest<HTMLButtonElement>("button")?.dataset.action
  const nodeId = target.closest<HTMLElement>("[data-node-id]")?.dataset.nodeId
  if (nodeId) { selected = new Set([nodeId]); render(); void sceneClient.saveSelection(scene.revision, [...selected]).catch(showError); return }
  if (action === "add") void addText().catch(showError)
  if (action === "html-node") void addContentNode("iyw:html").catch(showError)
  if (action === "markdown-node") void addContentNode("markdown:doc").catch(showError)
  if (action === "svg-node") void addContentNode("svg:vector").catch(showError)
  if (action === "slides-node") void addContentNode("iyw:slides").catch(showError)
  if (action === "edit") void editSelected().catch(showError)
  if (action === "retry") void creative.retrySelected().catch(showError)
  if (action === "export") void exportHtml().catch(showError)
  if (action === "export-slides") void creative.exportSelectedSlides().then((value) => { statusElement.textContent = value.missing.length ? `Export ${value.exportId}; missing ${value.missing.join(", ")}` : `Export ${value.exportId} complete` }).catch(showError)
  if (action === "export-svg") void exportRendered("svg").catch(showError)
  if (action === "export-png") void exportRendered("png").catch(showError)
  if (action === "annotate") void addAnnotation().catch(showError)
  if (action === "present") void presentSelected().catch(showError)
  if (action === "image") void creative.request("image.generate").catch(showError)
  if (action === "html") {
    const target = scene.nodes.find((item) => selected.has(item.id) && ["iyw:html", "html:render"].includes(item.type))
    void creative.request(target ? "html.edit" : "html.generate", undefined, target?.id).catch(showError)
  }
  if (action === "slides") void creative.request("slides.generate").catch(showError)
  if (action === "refresh") void refresh().catch(showError)
  if (action === "retry-connect") void start().catch(showError)
  if (action === "fullscreen") void app.requestDisplayMode({ mode: "fullscreen" }).catch(showError)
})
app.ontoolinput = (input) => {
  if (typeof input.arguments?.canvasId === "string") {
    if (input.arguments.canvasId !== canvasId) {
      selected = new Set()
      sceneLoaded = false
      sceneEpoch += 1
      scene = { ...scene, canvasId: input.arguments.canvasId, revision: 0, nodes: [], connections: [], viewport: { x: 0, y: 0, k: 1 } }
    }
    canvasId = input.arguments.canvasId
    void refresh().catch(showError)
  }
}
app.ontoolresult = (result) => { void creative.handleToolResult(result).catch(showError) }
let stopPolling: () => void = () => undefined
app.onteardown = async () => { stopPolling(); presenterCleanup?.(); await gestureBatcher.dispose(); client.dispose(); return {} }
let started = false
async function start() { if (started) return; started = true; try { await app.connect(); await refresh(); const storedSelection = await sceneClient.readSelection(); selected = new Set(storedSelection.selectedNodeIds.filter((id) => scene.nodes.some((node) => node.id === id))); render(); stopPolling = client.startPolling(refresh); window.addEventListener("unload", () => { stopPolling(); presenterCleanup?.(); void gestureBatcher.dispose(); client.dispose() }, { once: true }) } catch (error) { started = false; showError(error) } }
void start().catch(showError)
