export type DisplayImageSourceKind = "file" | "url"

export interface DisplayImageMetadata {
  caption: string | null
  name: string
  sourceKind: DisplayImageSourceKind | null
  uri: string
  mimeType: string
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value)
}

export function parseDisplayImageMetadata(
  text: string | null | undefined
): DisplayImageMetadata | null {
  if (!text?.trim()) return null

  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return null
  }
  value = findMetadata(value)
  if (!isRecord(value)) return null

  const caption = value.caption ?? null
  const sourceKind = value.source_kind ?? null
  const uri = value.uri
  const mimeType = value.mime_type
  if (typeof value.name !== "string" || !value.name.trim()) return null
  if (caption !== null && typeof caption !== "string") return null
  if (sourceKind !== null && sourceKind !== "file" && sourceKind !== "url") {
    return null
  }
  if (typeof uri !== "string" || !uri.trim()) return null
  if (typeof mimeType !== "string" || !mimeType.startsWith("image/"))
    return null

  return {
    caption,
    name: value.name,
    sourceKind,
    uri,
    mimeType,
  }
}

function findMetadata(value: unknown): unknown {
  if (isRecord(value) && value.type === "iyw_claw_display_image") return value
  if (typeof value === "string") {
    try {
      return findMetadata(JSON.parse(value))
    } catch {
      return null
    }
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      const found = findMetadata(item)
      if (found) return found
    }
  }
  if (isRecord(value)) {
    for (const key of [
      "structuredContent",
      "structured_content",
      "content",
      "text",
    ]) {
      const found = findMetadata(value[key])
      if (found) return found
    }
  }
  return null
}
