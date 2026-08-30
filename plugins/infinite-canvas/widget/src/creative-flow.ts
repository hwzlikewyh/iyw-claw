import type { App } from "@modelcontextprotocol/ext-apps"
import type { AssetClient } from "./asset-client.js"
import type { CanvasClient } from "./canvas-client.js"
import { applyCreativeResult, newCreativeRequest, sendCreativeRequest, type CreativeAction, type CreativeResultV1 } from "./creative-request.js"
import { exportDeckHtml, exportDeckPages } from "./plugins/slides/slides-export.js"
import type { SlideDeck } from "./plugins/slides/types.js"
import type { NodeData, Scene } from "./scene-renderer.js"

type CreativeFlowOptions = {
  app: App
  client: CanvasClient
  assets: AssetClient
  getScene: () => Scene
  getSelected: () => ReadonlySet<string>
  setScene: (scene: Scene) => void
  render: () => void
  markMutation: () => void
  report: (error: unknown) => void
}

export class CreativeFlow {
  constructor(private readonly options: CreativeFlowOptions) {}

  async request(action: CreativeAction, requestedPrompt?: string, targetNodeId?: string): Promise<void> {
    const scene = this.options.getScene()
    const prompt = (requestedPrompt ?? window.prompt("Prompt", defaultPrompt(action)))?.trim()
    if (!prompt) return
    const request = newCreativeRequest(action, scene.canvasId, prompt, [...this.options.getSelected()], targetNodeId, scene.revision)
    const pending: NodeData = { id: `request-${request.requestId.slice(0, 8)}`, type: "creative-request", x: 120, y: 240 + scene.nodes.length * 8, width: 300, height: 72, metadata: { requestId: request.requestId, action, prompt, status: "pending", selectionNodeIds: request.selectionNodeIds, ...(targetNodeId ? { targetNodeId, targetBaseRevision: scene.revision } : {}) } }
    try {
      const next = await this.options.client.apply([{ type: "add_node", node: pending }], scene.revision)
      this.options.markMutation()
      this.options.setScene(next as Scene)
      this.options.render()
      await sendCreativeRequest(this.options.app, request)
    } catch (error) {
      await this.markFailed(pending, error).catch(() => undefined)
      this.options.report(error)
    }
  }

  async retrySelected(): Promise<void> {
    const scene = this.options.getScene()
    const node = scene.nodes.find((item) => this.options.getSelected().has(item.id))
    const metadata = node?.metadata
    if (!node || node.type !== "creative-request" || metadata?.status !== "error" || typeof metadata.action !== "string" || typeof metadata.prompt !== "string") return
    if (!isCreativeAction(metadata.action)) return
    await this.request(metadata.action, metadata.prompt, typeof metadata.targetNodeId === "string" ? metadata.targetNodeId : undefined)
  }

  async exportSelectedSlides(): Promise<{ exportId: string; missing: string[] }> {
    const scene = this.options.getScene()
    const node = scene.nodes.find((item) => this.options.getSelected().has(item.id))
    const deck = node?.type === "iyw:slides" ? node.metadata?.deck : undefined
    if (!deck || typeof deck !== "object") throw new Error("select a slide deck first")
    const typedDeck = deck as SlideDeck
    const source = new Blob([exportDeckHtml(typedDeck)], { type: "text/html" })
    const uploaded = await this.options.client.upload(source, `slides-${scene.canvasId}.html`, "text/html")
    const exportId = `slides-${crypto.randomUUID().slice(0, 12)}`
    await this.options.client.callTool("export_infinite_canvas", { canvasId: scene.canvasId, format: "html", sourceAssetSha256: uploaded.sha256, exportId, fileName: `slides-${scene.canvasId}.html` })
    const pages = await exportDeckPages(typedDeck, this.options.assets)
    const missing = [...pages.missing]
    for (const page of pages.pages) {
      try { await this.options.client.callTool("export_infinite_canvas", { canvasId: scene.canvasId, format: "png", sourceAssetSha256: page.asset.sha256, exportId, fileName: `${page.pageId}.png` }) }
      catch { missing.push(page.pageId) }
    }
    return { exportId, missing: [...new Set(missing)] }
  }

  async handleToolResult(result: unknown): Promise<void> {
    const value = parseCreativeResult(result)
    if (!value) return
    const scene = this.options.getScene()
    const next = await applyCreativeResult(this.options.client, this.options.assets, scene, value)
    this.options.markMutation()
    this.options.setScene(next as Scene)
    this.options.render()
  }

  async send(request: Parameters<typeof sendCreativeRequest>[1]): Promise<void> { await sendCreativeRequest(this.options.app, request) }

  async markNodeFailed(nodeId: string, error: unknown): Promise<void> {
    const scene = this.options.getScene()
    const node = scene.nodes.find((item) => item.id === nodeId)
    if (node) await this.markFailed(node, error)
  }

  private async markFailed(node: NodeData, error: unknown): Promise<void> {
    const scene = this.options.getScene()
    if (!scene.nodes.some((item) => item.id === node.id)) return
    const next = await this.options.client.apply([{ type: "update_node", nodeId: node.id, patch: { metadata: { ...(node.metadata ?? {}), status: "error", errorCode: error instanceof Error ? error.message : "request_failed" } } }], scene.revision)
    this.options.markMutation()
    this.options.setScene(next as Scene)
    this.options.render()
  }

}

function defaultPrompt(action: CreativeAction): string { return action === "image.generate" ? "Create an image" : action === "slides.generate" ? "Create a slide deck" : "Create a web draft" }
function isCreativeAction(value: string): value is CreativeAction { return ["image.generate", "image.annotation-edit", "html.generate", "html.edit", "slides.generate", "slides.annotation-edit"].includes(value) }

function parseCreativeResult(result: unknown): CreativeResultV1 | undefined {
  const candidate = result && typeof result === "object" ? result as Record<string, unknown> : {}
  const structured = candidate.structuredContent && typeof candidate.structuredContent === "object" ? candidate.structuredContent : undefined
  const text = Array.isArray(candidate.content) ? (candidate.content as Array<{ type?: string; text?: string }>).find((item) => item.type === "text")?.text : undefined
  const raw = structured ?? (text ? tryParse(text) : undefined)
  if (!raw || typeof raw !== "object") return undefined
  const value = raw as Record<string, unknown>
  if (value.schemaVersion !== 1 || typeof value.requestId !== "string" || typeof value.action !== "string" || !["success", "partial", "error"].includes(String(value.status))) return undefined
  return value as unknown as CreativeResultV1
}

function tryParse(text: string): unknown { try { return JSON.parse(text) } catch { return undefined } }
