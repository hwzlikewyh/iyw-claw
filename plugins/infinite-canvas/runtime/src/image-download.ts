import { createHash } from "node:crypto"
import { lookup } from "node:dns/promises"
import { get } from "node:https"
import { BlockList } from "node:net"
import { appStorage } from "./app-storage.js"
import type { AssetStore } from "./asset-store.js"
import type { AssetRef } from "./types.js"
import { CanvasRuntimeError } from "./errors.js"
import { imageDownloadUrl, MAX_IMAGE_DOWNLOAD_BYTES } from "../../shared/image-download-policy.js"

const DOWNLOAD_TIMEOUT_MS = 120_000
const CHUNK_BYTES = 128 * 1024
const CACHE_NAMESPACE = "downloaded-images"
const reserved = new BlockList()
for (const [ip, bits] of [["0.0.0.0", 8], ["10.0.0.0", 8], ["100.64.0.0", 10], ["127.0.0.0", 8], ["169.254.0.0", 16], ["172.16.0.0", 12], ["192.168.0.0", 16], ["192.0.0.0", 24], ["192.0.2.0", 24], ["198.18.0.0", 15], ["198.51.100.0", 24], ["203.0.113.0", 24], ["224.0.0.0", 3]] as const) reserved.addSubnet(ip, bits, "ipv4")
for (const [ip, bits] of [["::", 128], ["::1", 128], ["fc00::", 7], ["fe80::", 10], ["ff00::", 8], ["2001:db8::", 32]] as const) reserved.addSubnet(ip, bits, "ipv6")
const pending = new Map<string, Promise<AssetRef>>()

export async function downloadImage(sourceUrl: string, assets: AssetStore): Promise<AssetRef> {
  const url = imageDownloadUrl(sourceUrl)
  const key = createHash("sha256").update(url.href).digest("hex")
  const existing = pending.get(key)
  if (existing) return existing
  const operation = downloadAndStore(url, key, assets)
  pending.set(key, operation)
  try { return await operation } finally { pending.delete(key) }
}

async function downloadAndStore(url: URL, key: string, assets: AssetStore): Promise<AssetRef> {
  try {
    const { value } = await appStorage({ operation: "get", namespace: CACHE_NAMESPACE, key }) as { value?: AssetRef }
    if (value?.sha256) {
      try { await assets.readChunk(value.sha256, 0, 1); return value } catch { /* 缓存丢失时仅重新下载图片 */ }
    }
    const data = await downloadBytes(url)
    const mimeType = imageMimeType(data)
    const name = `generated.${mimeType.split("/")[1]}`
    const expected = createHash("sha256").update(data).digest("hex")
    const { uploadId } = await assets.begin(name, mimeType, data.length, expected)
    let asset: AssetRef
    try {
      for (let offset = 0; offset < data.length; offset += CHUNK_BYTES) await assets.writeChunk(uploadId, offset / CHUNK_BYTES, data.subarray(offset, offset + CHUNK_BYTES).toString("base64"))
      asset = await assets.finalize(uploadId)
    } catch (error) { await assets.cancel(uploadId); throw error }
    await appStorage({ operation: "set", namespace: CACHE_NAMESPACE, key, value: asset })
    console.error(JSON.stringify({ event: "canvas_image_download", status: "saved", bytes: asset.bytes, mimeType }))
    return asset
  } catch (error) {
    console.error(JSON.stringify({ event: "canvas_image_download", status: "failed", reason: error instanceof CanvasRuntimeError ? error.message : "download_unavailable" }))
    throw new CanvasRuntimeError("asset_download_failed", "图片已生成，但下载或保存失败。请重试下载，不要重新生成。")
  }
}

async function downloadBytes(url: URL): Promise<Buffer> {
  const addresses = await lookup(url.hostname, { all: true })
  if (!addresses.length || addresses.some(({ address, family }) => reserved.check(address, family === 4 ? "ipv4" : "ipv6"))) throw failure("image_address_rejected")
  const address = addresses[0]!
  return new Promise((resolve, reject) => {
    const request = get(url, { family: address.family, headers: { Accept: "image/png,image/jpeg,image/webp,image/gif" }, lookup: (_host, options, done) => {
      if (options.all) done(null, [address]); else done(null, address.address, address.family)
    } }, (response) => {
      if (response.statusCode !== 200 || Number(response.headers["content-length"] || 0) > MAX_IMAGE_DOWNLOAD_BYTES) { response.destroy(); reject(failure("image_http_rejected")); return }
      const chunks: Buffer[] = []; let bytes = 0
      response.on("data", (chunk: Buffer) => {
        bytes += chunk.length
        if (bytes > MAX_IMAGE_DOWNLOAD_BYTES) { response.destroy(failure("image_too_large")); return }
        chunks.push(chunk)
      })
      response.once("end", () => resolve(Buffer.concat(chunks)))
      response.once("error", reject)
      response.once("aborted", () => reject(failure("image_download_interrupted")))
    })
    const timer = setTimeout(() => request.destroy(failure("image_download_timeout")), DOWNLOAD_TIMEOUT_MS)
    request.once("close", () => clearTimeout(timer))
    request.once("error", reject)
  })
}

function imageMimeType(data: Buffer): string {
  if (data.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) return "image/png"
  if (data[0] === 255 && data[1] === 216 && data[2] === 255) return "image/jpeg"
  if (["GIF87a", "GIF89a"].includes(data.subarray(0, 6).toString())) return "image/gif"
  if (data.subarray(0, 4).toString() === "RIFF" && data.subarray(8, 12).toString() === "WEBP") return "image/webp"
  throw failure("image_content_invalid")
}
function failure(message: string) { return new CanvasRuntimeError("asset_download_failed", message) }
