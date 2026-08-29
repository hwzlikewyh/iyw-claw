import { readFile, stat } from "node:fs/promises"
import { join } from "node:path"
import { CanvasRuntimeError } from "./errors.js"
import { pluginRoot } from "./paths.js"
import { resourceUri } from "./contracts.js"

const MAX_RESOURCE_BYTES = 8 * 1024 * 1024

export async function listResources() {
  return { resources: [{ uri: resourceUri, name: "Infinite Canvas", description: "Project-persisted Infinite Canvas widget", mimeType: "text/html;profile=mcp-app" }] }
}

export async function readResource(uri: string) {
  if (uri !== resourceUri) throw new CanvasRuntimeError("resource_not_found", "resource URI is not registered")
  const path = join(pluginRoot(), "widget", "dist", "infinite-canvas-widget.html")
  const info = await stat(path).catch(() => undefined)
  if (!info || !info.isFile() || info.size > MAX_RESOURCE_BYTES) throw new CanvasRuntimeError("resource_not_found", "widget resource is unavailable")
  const text = await readFile(path, "utf8")
  return { contents: [{ uri, mimeType: "text/html;profile=mcp-app", text, _meta: { "ui.csp": { connectDomains: [], resourceDomains: [], frameDomains: [] }, "ui.permissions": { clipboardWrite: true } } }] }
}
