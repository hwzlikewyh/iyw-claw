import type {
  EditorImageResult,
  ImageEditorCanvasHandle,
} from "./image-editor-model"

export interface InlineImageData {
  data: string
  mime_type: string
  suggestedName: string
}

export type CanvasExportResult =
  | { status: "ok"; result: EditorImageResult }
  | { status: "not-ready" }
  | { status: "tainted" }

export function exportCanvasImage(
  canvas: ImageEditorCanvasHandle | null,
  alt: string,
  edited: boolean
): CanvasExportResult {
  const outcome = canvas?.exportPng()
  if (!outcome || outcome.status === "not-ready") {
    return { status: "not-ready" }
  }
  if (outcome.status === "tainted") return { status: "tainted" }
  const comma = outcome.dataUrl.indexOf(",")
  if (comma < 0) return { status: "not-ready" }
  const base = alt.replace(/\.[^.]+$/, "").trim() || "image"
  return {
    status: "ok",
    result: {
      data: outcome.dataUrl.slice(comma + 1),
      mime_type: "image/png",
      name: `${base}${edited ? "-annotated" : ""}.png`,
    },
  }
}

function imageExtension(mimeType: string): string {
  const subtype = mimeType.split("/")[1]?.split("+")[0] || "png"
  return subtype === "jpeg" ? "jpg" : subtype
}

function suggestedImageName(alt: string, mimeType: string): string {
  const extension = imageExtension(mimeType)
  const trimmed = alt.trim()
  if (!trimmed) return `image.${extension}`
  const base = trimmed.replace(/\.[a-z0-9]{1,10}$/i, "").trim() || "image"
  return `${base}.${extension}`
}

export function parseInlineImage(
  src: string,
  alt: string
): InlineImageData | null {
  const comma = src.indexOf(",")
  if (comma < 0 || !src.startsWith("data:")) return null
  const [rawMimeType, ...rawParameters] = src.slice(5, comma).split(";")
  const mimeType = rawMimeType.toLowerCase()
  const parameters = rawParameters.map((parameter) => parameter.toLowerCase())
  const data = src.slice(comma + 1)
  if (
    !mimeType.startsWith("image/") ||
    !parameters.includes("base64") ||
    !data
  ) {
    return null
  }
  return {
    data,
    mime_type: mimeType,
    suggestedName: suggestedImageName(alt, mimeType),
  }
}

// `btoa` only accepts a binary string, and `String.fromCharCode(...bytes)`
// hits the call-stack limit somewhere around a few hundred KB. Chunk the
// buffer so multi-MB images encode without blowing the stack.
function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer)
  let binary = ""
  const chunkSize = 0x8000
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const slice = bytes.subarray(i, i + chunkSize) as unknown as number[]
    binary += String.fromCharCode.apply(null, slice)
  }
  return btoa(binary)
}

/**
 * Fetch the original bytes of a non-inline image (remote URL) so an
 * unedited export doesn't need the canvas at all — the canvas would
 * re-encode to PNG and, for hosts without CORS headers, is tainted and
 * cannot export at all.
 */
export async function fetchInlineImage(
  src: string,
  alt: string
): Promise<InlineImageData> {
  const response = await fetch(src)
  if (!response.ok) {
    throw new Error(`Failed to fetch image (HTTP ${response.status})`)
  }
  const blob = await response.blob()
  if (blob.size === 0) throw new Error("Fetched image is empty")
  const mimeType = blob.type.toLowerCase()
  if (!mimeType.startsWith("image/")) {
    throw new Error("Fetched resource is not an image")
  }
  const data = arrayBufferToBase64(await blob.arrayBuffer())
  return {
    data,
    mime_type: mimeType,
    suggestedName: suggestedImageName(alt, mimeType),
  }
}
