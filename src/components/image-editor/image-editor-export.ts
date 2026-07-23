import type {
  EditorImageResult,
  ImageEditorCanvasHandle,
} from "./image-editor-model"

export interface InlineImageData {
  data: string
  mime_type: string
  suggestedName: string
}

export function exportCanvasImage(
  canvas: ImageEditorCanvasHandle | null,
  alt: string,
  edited: boolean
): EditorImageResult | null {
  const dataUrl = canvas?.exportPng()
  if (!dataUrl) return null
  const comma = dataUrl.indexOf(",")
  if (comma < 0) return null
  const base = alt.replace(/\.[^.]+$/, "").trim() || "image"
  return {
    data: dataUrl.slice(comma + 1),
    mime_type: "image/png",
    name: `${base}${edited ? "-annotated" : ""}.png`,
  }
}

export function parseInlineImage(
  src: string,
  alt: string
): InlineImageData | null {
  const comma = src.indexOf(",")
  if (comma < 0 || !src.startsWith("data:")) return null
  const [mimeType, ...parameters] = src.slice(5, comma).split(";")
  const data = src.slice(comma + 1)
  if (
    !mimeType.startsWith("image/") ||
    !parameters.includes("base64") ||
    !data
  ) {
    return null
  }
  const subtype = mimeType.split("/")[1]?.split("+")[0] || "png"
  const extension = subtype === "jpeg" ? "jpg" : subtype
  return {
    data,
    mime_type: mimeType,
    suggestedName: alt.trim() || `image.${extension}`,
  }
}
