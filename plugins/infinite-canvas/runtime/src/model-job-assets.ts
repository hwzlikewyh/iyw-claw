import type { AssetStore } from "./asset-store.js"
import { invalid } from "./errors.js"

const CHUNK_BYTES = 128 * 1024
const MAX_IMAGE_BYTES = 32 * 1024 * 1024

// 将编辑请求中的内嵌图片保存为工作区资产，避免把 Base64 放进会话提示词。
export async function prepareModelInput(value: unknown, assets: AssetStore): Promise<unknown> {
  if (typeof value === "string" && value.startsWith("data:image/")) return persistInputImage(value, assets)
  if (Array.isArray(value)) {
    const result = []
    for (const item of value) result.push(await prepareModelInput(item, assets))
    return result
  }
  if (value && typeof value === "object") {
    const result: Record<string, unknown> = {}
    for (const [key, item] of Object.entries(value)) result[key] = await prepareModelInput(item, assets)
    return result
  }
  return value
}

async function persistInputImage(value: string, assets: AssetStore): Promise<string> {
  const match = /^data:(image\/(?:png|jpeg|webp|gif));base64,([A-Za-z0-9+/=\r\n]+)$/.exec(value)
  if (!match) throw invalid("model_input_image_invalid")
  const bytes = Buffer.from(match[2]!, "base64")
  if (!bytes.length || bytes.length > MAX_IMAGE_BYTES) throw invalid("model_input_image_size_invalid")
  const { createHash } = await import("node:crypto")
  const extension = match[1]!.split("/")[1]
  const upload = await assets.begin(`model-input.${extension}`, match[1]!, bytes.length, createHash("sha256").update(bytes).digest("hex"))
  try {
    for (let offset = 0; offset < bytes.length; offset += CHUNK_BYTES) await assets.writeChunk(upload.uploadId, offset / CHUNK_BYTES, bytes.subarray(offset, offset + CHUNK_BYTES).toString("base64"))
    const asset = await assets.finalize(upload.uploadId)
    return `canvas/infinite-canvas/${asset.path}`
  } catch (error) { await assets.cancel(upload.uploadId); throw error }
}
