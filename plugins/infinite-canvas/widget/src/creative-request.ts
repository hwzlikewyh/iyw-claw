import type { App } from "@modelcontextprotocol/ext-apps"
import type { AssetClient, AssetReference } from "./asset-client.js"
import type { CanvasOperation, CanvasState } from "./canvas-client.js"
import { sanitizeHtml } from "./plugins/html/html-actions.js"
import { validateDeck, type SlideDeck } from "./plugins/slides/types.js"

export type CreativeAction = "image.generate" | "image.annotation-edit" | "html.generate" | "html.edit" | "slides.generate" | "slides.annotation-edit"
export type CreativeRequestV1 = {
  schemaVersion: 1
  requestId: string
  action: CreativeAction
  canvasId: string
  revision: number
  targetNodeId?: string
  prompt: string
  selectionNodeIds: string[]
  assetPaths: string[]
  inputAssetSha256?: string
}

export type CreativeOutput = {
  kind: "image" | "html" | "slides"
  sourcePath?: string
  dataBase64?: string
  mimeType?: string
  name?: string
  bytes?: number
  sha256?: string
  width?: number
  height?: number
  content?: string
  deck?: Record<string, unknown>
}

export type CreativeResultV1 = {
  schemaVersion: 1
  requestId: string
  action: CreativeAction
  status: "success" | "partial" | "error"
  outputs?: CreativeOutput[]
  errorCode?: string
}

export async function sendCreativeRequest(app: App, request: CreativeRequestV1): Promise<void> {
  const text = `请在 Infinite Canvas 中执行 ${request.action}。\n\n\`\`\`json\n${JSON.stringify(request, null, 2)}\n\`\`\``
  const result = await app.sendMessage({ role: "user", content: [{ type: "text", text }] })
  if (result.isError) throw new Error("Agent rejected the creative request")
}

export function newCreativeRequest(action: CreativeAction, canvasId: string, prompt: string, selectionNodeIds: string[], targetNodeId?: string, revision = 0, assetPaths: string[] = []): CreativeRequestV1 {
  return { schemaVersion: 1, requestId: crypto.randomUUID(), action, canvasId, revision, prompt, selectionNodeIds, assetPaths, ...(targetNodeId ? { targetNodeId } : {}) }
}

export async function applyCreativeResult(client: CanvasClientLike, assets: AssetClient, scene: CanvasState, result: CreativeResultV1): Promise<CanvasState> {
  const requestNode = scene.nodes.find((node) => (node.metadata as Record<string, unknown> | undefined)?.requestId === result.requestId)
  if (!requestNode) return scene
  const requestMetadata = requestNode.metadata as Record<string, unknown> | undefined
  if (requestMetadata?.action !== result.action) return scene
  if (result.status === "error") return client.apply([{ type: "update_node", nodeId: requestNode.id, patch: { metadata: { ...(requestNode.metadata as Record<string, unknown> ?? {}), status: "error", errorCode: result.errorCode ?? "request_failed" } } }], scene.revision)
  const operations: CanvasOperation[] = []
  const outputs = result.outputs ?? []
  let targetUpdate: CanvasOperation | undefined
  try { targetUpdate = targetCreativeUpdate(scene, requestNode, result, outputs) }
  catch (error) {
    return client.apply([{ type: "update_node", nodeId: requestNode.id, patch: { metadata: { ...(requestNode.metadata as Record<string, unknown> ?? {}), status: "error", errorCode: error instanceof Error ? error.message : "creative_target_conflict" } } }], scene.revision)
  }
  if (targetUpdate) {
    operations.push(targetUpdate)
    operations.push(requestStatus(requestNode, result.status))
    return client.apply(operations, scene.revision)
  }
  const origin = outputOrigin(scene, requestNode)
  let completed = 0
  let failureCode = result.errorCode
  for (const output of outputs) {
    try { operations.push(await outputOperation(assets, output, origin, completed, result)); completed += 1 }
    catch (error) { failureCode ??= error instanceof Error ? error.message : "creative_output_failed" }
  }
  const status = completed === 0 && failureCode ? "error" : failureCode ? "partial" : result.status
  operations.unshift({ type: "update_node", nodeId: requestNode.id, patch: { metadata: { ...(requestNode.metadata as Record<string, unknown> ?? {}), status, ...(failureCode ? { errorCode: failureCode } : {}) } } })
  return client.apply(operations, scene.revision)
}

type CanvasClientLike = { apply: (operations: CanvasOperation[], baseRevision: number) => Promise<CanvasState> }

async function outputOperation(assets: AssetClient, output: CreativeOutput, origin: { x: number; y: number }, index: number, result: CreativeResultV1): Promise<CanvasOperation> {
  const x = origin.x + index * 340
  if (output.kind === "html") {
    const content = sanitizeHtml(output.content ?? "")
    return { type: "add_node", node: node(`html-${result.requestId.slice(0, 8)}-${index}`, "iyw:html", x, origin.y, 320, 220, { content, source: content, status: "success", requestId: result.requestId }) }
  }
  if (output.kind === "slides") {
    if (!output.deck || typeof output.deck !== "object") throw new Error("creative slides output is missing a deck")
    validateDeck(output.deck as SlideDeck)
    return { type: "add_node", node: node(`slides-${result.requestId.slice(0, 8)}-${index}`, "iyw:slides", x, origin.y, 420, 260, { deck: output.deck, status: "success", requestId: result.requestId }) }
  }
  if (output.kind !== "image") throw new Error("creative output kind is invalid")
  const asset = await imageAsset(assets, output, result.requestId, index)
  const size = await imageSize(assets, asset)
  const scale = Math.min(1, 640 / size.width, 480 / size.height)
  return { type: "add_node", node: node(`image-${result.requestId.slice(0, 8)}-${index}`, "media", x, origin.y, Math.max(1, Math.round(size.width * scale)), Math.max(1, Math.round(size.height * scale)), { asset, requestId: result.requestId, status: "success", naturalWidth: size.width, naturalHeight: size.height }) }
}

async function imageSize(assets: AssetClient, asset: AssetReference): Promise<{ width: number; height: number }> {
  if (!asset.mimeType.startsWith("image/")) throw new Error("creative output is not an image")
  const url = await assets.getUrl(asset)
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => image.naturalWidth > 0 && image.naturalHeight > 0 ? resolve({ width: image.naturalWidth, height: image.naturalHeight }) : reject(new Error("creative image has no dimensions"))
    image.onerror = () => reject(new Error("creative image could not be decoded"))
    image.src = url
  })
}

async function imageAsset(assets: AssetClient, output: CreativeOutput, requestId: string, index: number): Promise<AssetReference> {
  if (output.sourcePath) return assets.importSource(output.sourcePath, output.name ?? `image-${requestId}-${index}.bin`, output.mimeType ?? "application/octet-stream")
  if (output.sha256 && Number.isSafeInteger(output.bytes) && typeof output.mimeType === "string") return { sha256: output.sha256, bytes: output.bytes as number, mimeType: output.mimeType }
  if (!output.dataBase64) throw new Error("creative image output is missing an asset")
  const bytes = Uint8Array.from(atob(output.dataBase64), (value) => value.charCodeAt(0))
  return assets.upload(new Blob([bytes], { type: output.mimeType ?? "image/png" }), output.name ?? `image-${requestId}-${index}.png`, output.mimeType ?? "image/png")
}

function outputOrigin(scene: CanvasState, requestNode: Record<string, unknown>): { x: number; y: number } {
  const x = typeof requestNode.x === "number" ? requestNode.x : 120
  const y = typeof requestNode.y === "number" ? requestNode.y : 240
  const nodes = scene.nodes as Array<{ id: string; x: number; y: number; width: number }>
  const metadata = requestNode.metadata as Record<string, unknown> | undefined
  const selection = Array.isArray(metadata?.selectionNodeIds) ? metadata.selectionNodeIds : []
  const selected = nodes.filter((node) => selection.includes(node.id))
  return selected.length ? { x: Math.max(...selected.map((node) => node.x + node.width)) + 32, y: Math.min(...selected.map((node) => node.y)) } : { x: x + 340, y }
}

function node(id: string, type: string, x: number, y: number, width: number, height: number, metadata: Record<string, unknown>): Record<string, unknown> {
  return { id, type, x, y, width, height, metadata }
}

function requestStatus(requestNode: Record<string, unknown>, status: string): CanvasOperation {
  return { type: "update_node", nodeId: String(requestNode.id), patch: { metadata: { ...(requestNode.metadata as Record<string, unknown> ?? {}), status } } }
}

function targetCreativeUpdate(scene: CanvasState, requestNode: Record<string, unknown>, result: CreativeResultV1, outputs: CreativeOutput[]): CanvasOperation | undefined {
  const metadata = requestNode.metadata as Record<string, unknown> | undefined
  const targetId = typeof metadata?.targetNodeId === "string" ? metadata.targetNodeId : undefined
  const target = targetId ? scene.nodes.find((node) => node.id === targetId) : undefined
  const output = outputs[0]
  if (!target || !output) return undefined
  if (typeof metadata?.targetBaseRevision === "number" && scene.revision !== metadata.targetBaseRevision + 1) throw new Error("creative target revision changed")
  if (result.action === "html.edit" && output.kind === "html") return { type: "update_node", nodeId: target.id, patch: { metadata: { ...(target.metadata ?? {}), content: sanitizeHtml(output.content ?? ""), source: sanitizeHtml(output.content ?? ""), status: "success", requestId: result.requestId } } }
  if (result.action !== "slides.annotation-edit" || output.kind !== "slides" || !output.deck || typeof output.deck !== "object") return undefined
  const current = (target.metadata as Record<string, unknown> | undefined)?.deck
  if (!current || typeof current !== "object") throw new Error("slides target deck is missing")
  validateDeck(output.deck as SlideDeck)
  const oldPages = (current as { pages?: unknown }).pages
  const nextPages = (output.deck as SlideDeck).pages
  if (!Array.isArray(oldPages) || oldPages.length !== nextPages.length || oldPages.some((page, index) => page && typeof page === "object" && (page as { id?: unknown }).id !== nextPages[index]?.id)) throw new Error("slides page revision changed")
  const changed = nextPages.filter((page, index) => JSON.stringify(page) !== JSON.stringify(oldPages[index])).length
  if (changed !== 1) throw new Error("slides annotation must update one page")
  return { type: "update_node", nodeId: target.id, patch: { metadata: { ...(target.metadata ?? {}), deck: output.deck, status: "success", requestId: result.requestId } } }
}
