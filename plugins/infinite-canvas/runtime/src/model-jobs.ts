import { randomUUID } from "node:crypto"
import { basename, join } from "node:path"
import { appStorage } from "./app-storage.js"
import type { AssetStore } from "./asset-store.js"
import { invalid } from "./errors.js"
import { withFileLock } from "./lock.js"
import { assertWithin, rejectSymlinkPath, storageRoot, workspacePath } from "./paths.js"
import { prepareModelInput } from "./model-job-assets.js"

const NAMESPACE = "model-jobs"
const JOB_TTL_MS = 30 * 60_000
const MAX_OUTPUT_IMAGES = 16
const PATHS = new Set(["models", "chat/completions", "responses", "responses/compact", "images/generations", "images/edits"])
type Job = { requestId: string; state: "pending" | "completed" | "failed" | "cancelled"; expiresAt: number; request: Record<string, unknown>; result?: unknown; error?: string }

export async function createModelJob(args: Record<string, unknown>, assets: AssetStore): Promise<unknown> {
  const path = String(args.path)
  if (!PATHS.has(path) || args.method !== (path === "models" ? "GET" : "POST")) throw invalid("model_request_invalid")
  if (args.method === "POST" && (!args.body || typeof args.body !== "object" || Array.isArray(args.body))) throw invalid("model_body_missing")
  const request = await prepareModelInput(args, assets) as Record<string, unknown>
  const job: Job = { requestId: randomUUID(), state: "pending", expiresAt: Date.now() + JOB_TTL_MS, request }
  await save(job)
  return { delegated: true, requestId: job.requestId }
}

export async function modelJob(args: Record<string, unknown>, assets: AssetStore): Promise<unknown> {
  const requestId = String(args.requestId || "")
  if (!/^[a-f0-9-]{36}$/.test(requestId)) throw invalid("model_request_id_invalid")
  const lockPath = join(storageRoot(), ".model-jobs.lock")
  await rejectSymlinkPath(lockPath)
  return withFileLock(lockPath, async () => {
    const { value } = await appStorage({ operation: "get", namespace: NAMESPACE, key: requestId }) as { value: Job | null }
    if (!value || value.requestId !== requestId) throw invalid("model_job_not_found")
    if (value.state === "pending" && Date.now() > value.expiresAt) {
      value.state = "failed"; value.error = "等待原助理生成超时，请在当前会话查看执行状态后再决定是否重试。"; await save(value)
    }
    return operate(value, args, assets)
  })
}

async function operate(job: Job, args: Record<string, unknown>, assets: AssetStore): Promise<unknown> {
  const operation = args.operation
  if (operation === "read") return { ...job, instructions: jobInstructions(job) }
  if (operation === "status") return status(job)
  if (!["complete", "fail", "cancel"].includes(String(operation))) throw invalid("model_job_operation_invalid")
  if (job.state !== "pending") return status(job)
  if (operation === "complete") {
    job.result = await complete(job, args, assets)
    job.state = "completed"
  } else {
    job.state = operation === "cancel" ? "cancelled" : "failed"
    job.error = typeof args.error === "string" ? args.error.slice(0, 2000) : "生成请求已取消"
  }
  await save(job)
  return status(job)
}

async function complete(job: Job, args: Record<string, unknown>, assets: AssetStore): Promise<unknown> {
  const path = String(job.request.path)
  if (path.startsWith("images/")) {
    const images = args.images
    if (!Array.isArray(images) || !images.length || images.length > MAX_OUTPUT_IMAGES) throw invalid("model_output_images_missing")
    const references = []
    for (const image of images) {
      if (!image || typeof image.sourcePath !== "string" || !image.sourcePath.replaceAll("\\", "/").startsWith("canvas/infinite-canvas/") || !/^image\/(png|jpeg|webp|gif)$/.test(image.mimeType)) throw invalid("model_output_image_invalid")
      assertWithin(storageRoot(), workspacePath(image.sourcePath))
      references.push(await assets.importSource(image.sourcePath, basename(image.sourcePath), image.mimeType))
    }
    return { images: references }
  }
  if (!args.response || typeof args.response !== "object" || Array.isArray(args.response)) throw invalid("model_response_missing")
  if (path === "models" && !Array.isArray((args.response as Record<string, unknown>).data)) throw invalid("model_catalog_data_missing")
  return { response: args.response }
}

function status(job: Job) { return { requestId: job.requestId, state: job.state, result: job.result, error: job.error } }
async function save(job: Job) {
  await appStorage({ operation: "set", namespace: NAMESPACE, key: job.requestId, value: job })
  console.error(JSON.stringify({ event: "canvas_model_job", requestId: job.requestId, state: job.state, path: job.request.path }))
}

function jobInstructions(job: Job): string {
  if (job.state !== "pending") return "该请求已结束，不要重新执行生成。"
  if (job.request.path === "models") return "只读同步原助理模型。使用已鉴权的 fetch_iyw_url GET https://gateway.iyw.cn/iyw-fusion-api/v1/models，query 使用 request.query（model_type=all）。凭据由宿主注入，禁止读取或填写 token。将真实接口响应 body 的 data 列表原样以 complete.response 回填，保留 id、display_name、capabilities；不填外层工具结果，不编造或静默补充模型；可省略价格字段。失败调用 fail。"
  return "这是用户在 Infinite Canvas 中提交的生成请求。图片必须使用原助理受授权的图片生成能力，保留提示词、参考图、尺寸、数量及选定模型；通过实际图片模型目录解析选定显示名，不得自行换模型。参考图片路径相对于工作区。输出存到 canvas/infinite-canvas/generated，complete.images 填相对路径与 MIME 类型。文本请求使用已鉴权的 fetch_iyw_url POST https://gateway.iyw.cn/iyw-fusion-api/v1/ 加 request.path，body 保留 request.body 的模型和参数，将 stream 设为 false；图片路径先经宿主上传工具转为 API 支持的图片引用。complete.response 回填真实接口响应 body，不用当前会话代写。禁止读取或传递账号凭据，不静默改模型或重试计费；失败调用 fail。"
}
