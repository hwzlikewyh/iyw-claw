export const CHAT_IMAGE_SOURCE_MAX_BYTES = 100 * 1024 * 1024
export const CHAT_IMAGE_INPUT_MAX_BYTES = 32 * 1024 * 1024
// 保留既有调用方名称，数值与原图输入上限保持一致。
export const CHAT_IMAGE_DERIVED_MAX_BYTES = CHAT_IMAGE_INPUT_MAX_BYTES
const CHAT_IMAGE_INSPECT_MAX_EDGE = 16_384
const BASE64_CHUNK_BYTES = 0x8000

export interface PreparedChatImage {
  data: string
  mimeType: string
  name: string
  sourceBytes: number
  derivedBytes: number
  width: number
  height: number
}

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer)
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += BASE64_CHUNK_BYTES) {
    binary += String.fromCharCode(
      ...bytes.subarray(offset, offset + BASE64_CHUNK_BYTES)
    )
  }
  return btoa(binary)
}

function originalImageMime(bytes: Uint8Array): string {
  const prefix = String.fromCharCode(...bytes.subarray(0, 12))
  if (prefix.startsWith("\x89PNG\r\n\x1a\n")) return "image/png"
  if (prefix.startsWith("\xff\xd8\xff")) return "image/jpeg"
  if (prefix.startsWith("GIF87a") || prefix.startsWith("GIF89a"))
    return "image/gif"
  if (prefix.startsWith("RIFF") && prefix.slice(8, 12) === "WEBP")
    return "image/webp"
  throw new Error("Image format is not supported")
}

function imageDimensions(file: File): Promise<[number, number]> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const image = new window.Image()
    image.onload = () => {
      URL.revokeObjectURL(url)
      resolve([image.naturalWidth, image.naturalHeight])
    }
    image.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error("Unable to inspect image"))
    }
    image.src = url
  })
}

export async function prepareBrowserChatImage(
  file: File
): Promise<PreparedChatImage> {
  if (file.size === 0) throw new Error("Image data is empty")
  if (file.size > CHAT_IMAGE_SOURCE_MAX_BYTES)
    throw new Error("Image exceeds the 100 MB source limit")
  if (file.size > CHAT_IMAGE_INPUT_MAX_BYTES)
    throw new Error(
      "Original image exceeds the current 32 MiB model input limit; the file has not been modified"
    )
  const buffer = await file.arrayBuffer()
  const mimeType = originalImageMime(new Uint8Array(buffer))
  const [width, height] = await imageDimensions(file)
  if (
    width <= 0 ||
    height <= 0 ||
    width > CHAT_IMAGE_INSPECT_MAX_EDGE ||
    height > CHAT_IMAGE_INSPECT_MAX_EDGE
  )
    throw new Error("Image dimensions are invalid")
  return {
    data: arrayBufferToBase64(buffer),
    mimeType,
    name: file.name || "image",
    sourceBytes: file.size,
    derivedBytes: buffer.byteLength,
    width,
    height,
  }
}
