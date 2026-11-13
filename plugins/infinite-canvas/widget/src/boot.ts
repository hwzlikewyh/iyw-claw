import { App } from "@modelcontextprotocol/ext-apps"
import { AssetClient } from "./asset-client.js"
import { installModelGatewayAdapter } from "./upstream/model-gateway-adapter.js"
import { installPluginStorage } from "./upstream/plugin-storage.js"
import "../../vendor/infinite-canvas-web/src/styles/globals.css"

async function start(): Promise<void> {
  const app = new App({ name: "Infinite Canvas", version: "0.1.18" }, { availableDisplayModes: ["inline", "fullscreen"] })
  await app.connect()
  const call = async (name: string, arguments_: Record<string, unknown>) => {
    const result = await app.callServerTool({ name, arguments: arguments_ })
    const text = result.content.find((part) => part.type === "text")?.text
    if (result.isError || !text) throw new Error(text || "Canvas host request failed")
    return JSON.parse(text) as Record<string, unknown>
  }
  const assets = new AssetClient(call)
  window.__IYW_CANVAS_BRIDGE__ = { app, assets }
  installModelGatewayAdapter(app)
  await installPluginStorage(app, assets)
  await import("@/plugin-main")
  app.onteardown = async () => { assets.dispose(); return {} }
}

void start().catch((error: unknown) => {
  const root = document.getElementById("root")
  if (root) root.textContent = error instanceof Error ? error.message : "Infinite Canvas could not start"
})
