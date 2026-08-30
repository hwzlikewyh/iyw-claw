import { createHash } from "node:crypto"
import { mkdtemp, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { fileURLToPath } from "node:url"
import { Client } from "@modelcontextprotocol/sdk/client/index.js"
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js"

const root = fileURLToPath(new URL("..", import.meta.url))
const temporaryRoot = await mkdtemp(join(tmpdir(), "iyw-infinite-canvas-"))
const workspace = join(temporaryRoot, "workspace")
const pluginData = join(temporaryRoot, "plugin-data")
const transport = new StdioClientTransport({
  command: process.execPath,
  args: [join(root, "runtime/dist/infinite-canvas-mcp.mjs")],
  env: {
    ...process.env,
    IYW_WORKSPACE_DIR: workspace,
    IYW_PLUGIN_DATA_DIR: pluginData,
    IYW_PLUGIN_ROOT: root,
  },
})
const client = new Client({ name: "infinite-canvas-smoke", version: "1.0.0" })

try {
  await client.connect(transport)
  const tools = await client.listTools()
  const resources = await client.listResources()
  const initial = text(await call("get_infinite_canvas_state", { canvasId: "smoke" }))
  const applied = text(await call("apply_infinite_canvas_ops", {
    canvasId: "smoke",
    baseRevision: initial.revision,
    operations: [
      { type: "add_node", node: { id: "note-1", type: "text", x: 32, y: 48, width: 240, height: 96, metadata: { text: "runtime smoke" } } },
      { type: "set_viewport", viewport: { x: 12, y: 24, k: 1.25 } },
    ],
  }))
  const bytes = Buffer.from("infinite-canvas-asset-smoke", "utf8")
  const sha256 = createHash("sha256").update(bytes).digest("hex")
  const started = text(await call("write_infinite_canvas_asset", { name: "smoke.txt", mimeType: "text/plain", expectedBytes: bytes.length, expectedSha256: sha256 }))
  await call("write_infinite_canvas_asset", { uploadId: started.uploadId, chunkIndex: 0, dataBase64: bytes.toString("base64") })
  const asset = text(await call("write_infinite_canvas_asset", { uploadId: started.uploadId, finalize: true }))
  const read = text(await call("read_infinite_canvas_asset", { sha256, offset: 0, length: 128 * 1024 }))
  const exported = text(await call("export_infinite_canvas", { canvasId: "smoke", format: "html", fileName: "smoke.html" }))
  const resource = await client.readResource({ uri: "ui://widget/infinite-canvas/canvas.html" })
  const widget = resource.contents[0]
  const result = {
    tools: tools.tools.length,
    resources: resources.resources.length,
    revision: applied.revision,
    nodes: applied.nodes.length,
    viewport: applied.viewport,
    assetSha256: asset.sha256,
    assetRoundTrip: Buffer.from(read.dataBase64, "base64").equals(bytes),
    exportPath: exported.relativePath,
    widgetBytes: typeof widget?.text === "string" ? Buffer.byteLength(widget.text) : 0,
  }
  assert(result)
  console.log(JSON.stringify(result, null, 2))
} finally {
  await client.close().catch(() => undefined)
  await rm(temporaryRoot, { recursive: true, force: true })
}

function call(name, args) { return client.callTool({ name, arguments: args }) }
function text(result) { return JSON.parse(result.content.find((part) => part.type === "text")?.text ?? "{}") }
function assert(result) {
  if (result.tools !== 10 || result.resources !== 1 || result.revision !== 1 || result.nodes !== 1 || !result.assetRoundTrip || result.widgetBytes < 1) throw new Error(`runtime smoke failed: ${JSON.stringify(result)}`)
}
