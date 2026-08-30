import { App } from "@modelcontextprotocol/ext-apps"

export function createCanvasApp() {
  return new App({ name: "Infinite Canvas", version: "0.1.9" }, { availableDisplayModes: ["inline", "fullscreen"] })
}
