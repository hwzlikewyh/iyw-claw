import { basename, join } from "node:path"
import { appStorage } from "./app-storage.js"
import type { AssetStore } from "./asset-store.js"
import { invalid } from "./errors.js"
import { withFileLock } from "./lock.js"
import { assertWithin, rejectSymlinkPath, storageRoot, workspacePath } from "./paths.js"

const NAMESPACE = "model-jobs"
const MAX_OUTPUT_IMAGES = 16
type Job = { requestId: string; state: "pending" | "completed" | "failed" | "cancelled"; expiresAt: number; request: Record<string, unknown>; result?: unknown; error?: string }

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
  return "这是旧版兼容任务。模型请求已改为当前窗口直接处理；不要开始新的生成或模型查询。尚未执行的请求请 cancel，已经完成的结果可 complete 保存。"
}
