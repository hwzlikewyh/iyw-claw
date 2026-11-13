import type { App } from "@modelcontextprotocol/ext-apps"
import type { AssetReference } from "../asset-client.js"
import type { GatewayResult } from "./model-gateway-adapter.js"
import type {} from "./host-bridge.js"

type Host = Pick<App, "callServerTool" | "sendMessage">
type JobStatus = { state: string; error?: string; result?: { images?: AssetReference[]; response?: Record<string, unknown> } }
type RequestContext = { path: string; body: unknown; signal?: { readonly aborted: boolean } }
const POLL_INTERVAL_MS = 1200
const GENERATION_TIMEOUT_MS = 30 * 60_000

export async function runLegacyModel(app: Host, requestId: string, context: RequestContext): Promise<GatewayResult> {
  if (context.signal?.aborted) { await job(app, requestId, "cancel"); throw new DOMException("Aborted", "AbortError") }
  const model = (context.body as { model?: string } | undefined)?.model
  const modelName = model && window.__IYW_CANVAS_BRIDGE__?.modelNames?.[model]
  const action = context.path === "models" ? "用户要求同步原助理模型列表，这是只读查询，不要执行生成。" : "用户已在 Infinite Canvas 提交生成请求，请执行并回填结果。"
  const text = `${action}请求 ID：${requestId}，类型：${context.path}。${modelName ? `选定模型：${modelName}。` : ""}\n先读取能力 plugin.infinite-canvas.host.model-job.v1 的完整 schema，再以 operation=read、requestId=${requestId} 读取请求和执行要求。完成后通过同一能力 operation=complete 回填，失败则 operation=fail；不要只在聊天里回复而不回填。已完成或已取消的请求不要再次执行。图片使用原助理已授权的生成工具，输出保存到 canvas/infinite-canvas/generated。其他请求使用原助理已鉴权的 fetch_iyw_url 调用任务给出的模型接口，保留选定模型，不用当前会话模型代答。不得读取或传递账号 token，不要静默重试计费。`
  const sent = await app.sendMessage({ role: "user", content: [{ type: "text", text }] })
  if (sent.isError) { await job(app, requestId, "cancel"); throw new Error("原助理未接受生成请求，请检查当前会话状态。") }
  if (context.path !== "models") notify(requestId, true)
  try { return await waitForResult(app, requestId, context) }
  finally { if (context.path !== "models") notify(requestId, false) }
}

async function waitForResult(app: Host, requestId: string, context: RequestContext): Promise<GatewayResult> {
  const deadline = Date.now() + GENERATION_TIMEOUT_MS
  while (Date.now() < deadline) {
    if (context.signal?.aborted) { await job(app, requestId, "cancel"); throw new DOMException("Aborted", "AbortError") }
    const status = await job(app, requestId, "status")
    if (status.state === "completed" && status.result) return toGatewayResult(status.result, context)
    if (status.state !== "pending") throw new Error(status.error || "原助理生成未完成，请查看当前会话。")
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS))
  }
  throw new Error("等待原助理生成超时，请先在当前会话检查执行结果，避免重复生成。")
}

function notify(requestId: string, pending: boolean): void {
  window.dispatchEvent(new CustomEvent("iyw-canvas-generation", { detail: { requestId, pending } }))
}

async function job(app: Host, requestId: string, operation: string): Promise<JobStatus> {
  const result = await app.callServerTool({ name: "infinite_canvas_model_job", arguments: { operation, requestId } })
  const text = result.content.find((item) => item.type === "text")?.text
  if (result.isError || !text) throw new Error("无法读取原助理生成进度，请在当前会话检查结果。")
  return JSON.parse(text) as JobStatus
}

async function toGatewayResult(result: NonNullable<JobStatus["result"]>, context: RequestContext): Promise<GatewayResult> {
  let response = result.response
  if (result.images?.length) {
    const assets = window.__IYW_CANVAS_BRIDGE__?.assets
    if (!assets) throw new Error("画布素材连接已失效，请重新打开插件。")
    const data = []
    for (const image of result.images) data.push({ url: await assets.getUrl(image) })
    response = { created: Math.floor(Date.now() / 1000), data }
  }
  if (!response) throw new Error("原助理未返回可用的生成结果。")
  if ((context.body as { stream?: boolean } | undefined)?.stream && context.path === "responses") {
    const event = { type: "response.completed", response }
    return { status: 200, contentType: "text/event-stream", body: { text: `event: response.completed\ndata: ${JSON.stringify(event)}\n\n` } }
  }
  return { status: 200, contentType: "application/json", body: { text: JSON.stringify(response) } }
}
