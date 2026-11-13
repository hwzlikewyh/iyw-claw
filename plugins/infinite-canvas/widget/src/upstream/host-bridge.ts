import type { App } from "@modelcontextprotocol/ext-apps"
import type { AssetClient } from "../asset-client.js"

export type HostWindowContext = { windowId: string; tabId: string; conversationId: number; agentLabel: string; modelId: string; modelName: string; modelGateway: boolean }
export type HostBridge = { app: App; assets: AssetClient; modelNames?: Record<string, string>; currentWindow?: HostWindowContext }

export function supportsCurrentWindow(app: Pick<App, "getHostCapabilities">): boolean {
  const feature = app.getHostCapabilities()?.experimental?.["iyw/current-window"] as { version?: unknown } | undefined
  return feature?.version === 1
}

export async function readCurrentWindow(app: App): Promise<HostWindowContext> {
  if (!supportsCurrentWindow(app)) throw new Error("hostAgent.unsupported")
  const result = await app.callServerTool({ name: "infinite_canvas_host_context", arguments: {} }, { timeout: 5000 })
  const text = result.content.find((part) => part.type === "text")?.text
  if (result.isError || !text) throw new Error("hostAgent.unavailable")
  const context = JSON.parse(text) as HostWindowContext
  if (!context.windowId || !context.tabId || !context.conversationId || !context.modelGateway) throw new Error("hostAgent.unsupported")
  return context
}

declare global { interface Window { __IYW_CANVAS_BRIDGE__?: HostBridge } }
