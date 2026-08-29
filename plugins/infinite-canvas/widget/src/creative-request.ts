import type { App } from "@modelcontextprotocol/ext-apps"

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

export async function sendCreativeRequest(app: App, request: CreativeRequestV1): Promise<void> {
  const text = `请在 Infinite Canvas 中执行 ${request.action}。\n\n\`\`\`json\n${JSON.stringify(request, null, 2)}\n\`\`\``
  const result = await app.sendMessage({ role: "user", content: [{ type: "text", text }] })
  if (result.isError) throw new Error("Agent rejected the creative request")
}

export function newCreativeRequest(action: CreativeAction, canvasId: string, prompt: string, selectionNodeIds: string[], targetNodeId?: string, revision = 0, assetPaths: string[] = []): CreativeRequestV1 {
  return { schemaVersion: 1, requestId: crypto.randomUUID(), action, canvasId, revision, prompt, selectionNodeIds, assetPaths, ...(targetNodeId ? { targetNodeId } : {}) }
}
