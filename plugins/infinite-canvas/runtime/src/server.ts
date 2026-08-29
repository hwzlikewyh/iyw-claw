import { Server } from "@modelcontextprotocol/sdk/server/index.js"
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js"
import { CallToolRequestSchema, ListResourcesRequestSchema, ListToolsRequestSchema, ReadResourceRequestSchema } from "@modelcontextprotocol/sdk/types.js"
import { contracts } from "./contracts.js"
import { createToolHandlers } from "./tool-handlers.js"
import { listResources, readResource } from "./resource.js"

export function createInfiniteCanvasServer() {
  const handlers = createToolHandlers()
  const server = new Server({ name: "infinite-canvas", version: "0.1.8" }, { capabilities: { tools: {}, resources: {} } })
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: Object.entries(contracts).map(([name, value]) => ({ name, description: value.description, inputSchema: value.inputSchema, annotations: { readOnlyHint: value.readOnlyHint } })) }))
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try { const result = await handlers.call(request.params.name, (request.params.arguments ?? {}) as Record<string, unknown>); return { content: [{ type: "text", text: JSON.stringify(result.data) }] } }
    catch (error) { const value = error instanceof Error ? error : new Error(String(error)); const payload = error && typeof error === "object" && "toJSON" in error ? (error as { toJSON: () => unknown }).toJSON() : { code: "runtime_unavailable", message: value.message }; return { isError: true, content: [{ type: "text", text: JSON.stringify(payload) }] } }
  })
  server.setRequestHandler(ListResourcesRequestSchema, async () => listResources())
  server.setRequestHandler(ReadResourceRequestSchema, async (request) => readResource(request.params.uri))
  return { server, close: handlers.close }
}

export async function runStdioServer(): Promise<void> {
  const runtime = createInfiniteCanvasServer()
  const stop = () => { void runtime.close(); void runtime.server.close() }
  process.once("SIGINT", stop)
  process.once("SIGTERM", stop)
  await runtime.server.connect(new StdioServerTransport())
}
