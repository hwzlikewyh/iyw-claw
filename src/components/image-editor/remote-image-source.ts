import { isDesktop } from "@/lib/transport"
import { getIywClawToken, getIywClawWebBaseUrl } from "@/lib/transport/web-auth"
import { notifyWebUnauthorized } from "@/lib/transport/web-connection-store"

const SUPPORTED_MIME_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
  "image/bmp",
])

type RawBinary = ArrayBuffer | Uint8Array | number[]

export function isRemoteImageUrl(source: string): boolean {
  return /^https?:\/\//i.test(source)
}

export async function fetchRemoteImageBlob(url: string): Promise<Blob> {
  if (!isRemoteImageUrl(url)) {
    throw new Error("Remote image URL must use HTTP or HTTPS")
  }
  return isDesktop() ? fetchDesktopImage(url) : fetchWebImage(url)
}

async function fetchDesktopImage(url: string): Promise<Blob> {
  const { invoke } = await import("@tauri-apps/api/core")
  const raw = await invoke<RawBinary>("fetch_remote_image", { url })
  const bytes = normalizeBytes(raw)
  if (bytes.byteLength === 0) throw new Error("Remote image is empty")
  return new Blob([bytes as BlobPart], { type: detectMimeType(bytes) })
}

async function fetchWebImage(url: string): Promise<Blob> {
  const response = await fetch(
    `${getIywClawWebBaseUrl()}/api/fetch_remote_image`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${getIywClawToken()}`,
      },
      body: JSON.stringify({ url }),
    }
  )
  if (response.status === 401) {
    notifyWebUnauthorized()
    throw new Error("Unauthorized")
  }
  if (!response.ok) throw await responseError(response)
  const mimeType = normalizeMimeType(response.headers.get("content-type"))
  const blob = await response.blob()
  if (blob.size === 0) throw new Error("Remote image is empty")
  return blob.type === mimeType ? blob : blob.slice(0, blob.size, mimeType)
}

async function responseError(response: Response): Promise<unknown> {
  return response.json().catch(() => ({
    code: "network_error",
    message: `HTTP ${response.status}`,
  }))
}

function normalizeBytes(raw: RawBinary): Uint8Array {
  if (raw instanceof Uint8Array) return raw
  if (raw instanceof ArrayBuffer) return new Uint8Array(raw)
  if (Array.isArray(raw)) return Uint8Array.from(raw)
  throw new Error("Remote image response is not binary")
}

function normalizeMimeType(value: string | null): string {
  const mimeType = value?.split(";", 1)[0].trim().toLowerCase() ?? ""
  if (!SUPPORTED_MIME_TYPES.has(mimeType)) {
    throw new Error("Remote response is not a supported image")
  }
  return mimeType
}

function detectMimeType(bytes: Uint8Array): string {
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47])) return "image/png"
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return "image/jpeg"
  if (ascii(bytes, 0, 3) === "GIF") return "image/gif"
  if (ascii(bytes, 0, 4) === "RIFF" && ascii(bytes, 8, 4) === "WEBP") {
    return "image/webp"
  }
  if (ascii(bytes, 0, 2) === "BM") return "image/bmp"
  throw new Error("Remote response is not a supported image")
}

function startsWith(bytes: Uint8Array, signature: number[]): boolean {
  return signature.every((value, index) => bytes[index] === value)
}

function ascii(bytes: Uint8Array, offset: number, length: number): string {
  if (bytes.length < offset + length) return ""
  return String.fromCharCode(...bytes.subarray(offset, offset + length))
}
