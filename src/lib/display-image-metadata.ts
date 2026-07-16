export type DisplayImageSourceKind = "file" | "url"

export interface DisplayImageMetadata {
  caption: string | null
  name: string
  sourceKind: DisplayImageSourceKind | null
  source: string | null
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
  if (!isRecord(value) || value.type !== "iyw_claw_display_image") {
    return null
  }

  const caption = value.caption ?? null
  const sourceKind = value.source_kind ?? null
  const source = value.source ?? null
  if (typeof value.name !== "string" || !value.name.trim()) return null
  if (caption !== null && typeof caption !== "string") return null
  if (sourceKind !== null && sourceKind !== "file" && sourceKind !== "url") {
    return null
  }
  if (source !== null && (typeof source !== "string" || !source.trim())) {
    return null
  }
  if ((sourceKind === null) !== (source === null)) return null

  return {
    caption,
    name: value.name,
    sourceKind,
    source,
  }
}
