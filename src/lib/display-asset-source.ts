import {
  getActiveRemoteConnectionId,
  isDesktop,
  notifyRemoteDesktopUnauthorized,
} from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"
import { getIywClawToken, getIywClawWebBaseUrl } from "@/lib/transport/web-auth"
import { notifyWebUnauthorized } from "@/lib/transport/web-connection-store"

const DISPLAY_ASSET_URI_PREFIX = "iyw-claw://display-assets/"
const HASH_PATTERN = /^[0-9a-f]{64}$/

type RawBinary = ArrayBuffer | Uint8Array | number[]

export function isDisplayAssetUri(uri: string | null | undefined): boolean {
  return parseDisplayAssetHash(uri) !== null
}

export async function fetchDisplayAsset(
  uri: string,
  expectedMimeType?: string | null
): Promise<Blob> {
  const hash = parseDisplayAssetHash(uri)
  if (!hash) throw new Error("Invalid display image URI")
  const remoteConnectionId = getActiveRemoteConnectionId()
  if (isDesktop())
    return fetchDesktopAsset(hash, remoteConnectionId, expectedMimeType)
  return fetchWebAsset(hash)
}

async function fetchDesktopAsset(
  hash: string,
  remoteConnectionId: number | null,
  expectedMimeType?: string | null
): Promise<Blob> {
  const { invoke } = await import("@tauri-apps/api/core")
  try {
    const raw = remoteConnectionId
      ? await invoke<RawBinary>("remote_read_display_asset", {
          connectionId: remoteConnectionId,
          hash,
        })
      : await invoke<RawBinary>("read_display_asset", { hash })
    const bytes = normalizeBytes(raw)
    return imageBlob(bytes, expectedMimeType)
  } catch (error) {
    if (
      remoteConnectionId &&
      extractAppCommandError(error)?.code === "authentication_failed"
    ) {
      notifyRemoteDesktopUnauthorized()
    }
    throw error
  }
}

async function fetchWebAsset(hash: string): Promise<Blob> {
  const response = await fetch(
    `${getIywClawWebBaseUrl()}/api/read_display_asset`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${getIywClawToken()}`,
      },
      body: JSON.stringify({ hash }),
    }
  )
  if (response.status === 401) {
    notifyWebUnauthorized()
    throw new Error("Unauthorized")
  }
  if (!response.ok) throw new Error(`HTTP ${response.status}`)
  const blob = await response.blob()
  if (!blob.type.startsWith("image/") || blob.size === 0) {
    throw new Error("Display image response is invalid")
  }
  return blob
}

function parseDisplayAssetHash(uri: string | null | undefined): string | null {
  if (!uri?.startsWith(DISPLAY_ASSET_URI_PREFIX)) return null
  const hash = uri.slice(DISPLAY_ASSET_URI_PREFIX.length)
  return HASH_PATTERN.test(hash) ? hash : null
}

function normalizeBytes(raw: RawBinary): Uint8Array {
  if (raw instanceof Uint8Array) return raw
  if (raw instanceof ArrayBuffer) return new Uint8Array(raw)
  if (Array.isArray(raw)) return Uint8Array.from(raw)
  throw new Error("Display image response is not binary")
}

function imageBlob(bytes: Uint8Array, expectedMimeType?: string | null): Blob {
  if (bytes.byteLength === 0) throw new Error("Display image is empty")
  const mimeType = normalizeImageMimeType(expectedMimeType)
  if (!mimeType) throw new Error("Display image format is unsupported")
  return new Blob([bytes as BlobPart], { type: mimeType })
}

function normalizeImageMimeType(value?: string | null): string | null {
  const mimeType = value?.split(";", 1)[0]?.trim().toLowerCase()
  switch (mimeType) {
    case "image/png":
    case "image/jpeg":
    case "image/gif":
    case "image/webp":
    case "image/bmp":
    case "image/avif":
    case "image/svg+xml":
      return mimeType
    default:
      return null
  }
}
