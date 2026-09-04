export interface DeliveredImage {
  uri: string
  mimeType: string
}

export function extractDeliveredImage(
  output: string | null | undefined
): DeliveredImage | null {
  const parsed = parseRecord(output)
  if (!parsed) return null

  const result = parseRecordValue(parsed.result)
  const structured = parseRecordValue(
    parsed.structuredContent ?? parsed.structured_content
  )
  const resultStructured = parseRecordValue(
    result?.structuredContent ?? result?.structured_content
  )
  const roots = [parsed, result, structured, resultStructured]
  for (const root of roots) {
    const delivery = parseRecordValue(root?.delivery)
    const artifact = parseRecordValue(delivery?.artifact)
    const accepted = Array.isArray(artifact?.accepted) ? artifact.accepted : []
    for (const item of accepted) {
      const record = parseRecordValue(item)
      const uri = typeof record?.path === "string" ? record.path.trim() : ""
      if (!/^https?:\/\//i.test(uri)) continue
      return { uri, mimeType: imageMimeType(record?.display_name, uri) }
    }
    const images = Array.isArray(root?.images) ? root.images : []
    for (const item of images) {
      const imageRecord = parseRecordValue(item)
      const uri =
        typeof item === "string"
          ? item.trim()
          : typeof imageRecord?.url === "string"
            ? imageRecord.url.trim()
            : ""
      if (/^https?:\/\//i.test(uri)) {
        return { uri, mimeType: imageMimeType(null, uri) }
      }
    }
  }
  return null
}

function parseRecord(value: string | null | undefined) {
  if (!value?.trim()) return null
  try {
    return parseRecordValue(JSON.parse(value))
  } catch {
    for (const line of value.split(/\r?\n/).reverse()) {
      try {
        const parsed = parseRecordValue(JSON.parse(line.trim()))
        if (parsed) return parsed
      } catch {
        continue
      }
    }
    return null
  }
}

function parseRecordValue(value: unknown): Record<string, unknown> | null {
  if (typeof value === "string") {
    try {
      return parseRecordValue(JSON.parse(value))
    } catch {
      return null
    }
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) return null
  return value as Record<string, unknown>
}

function imageMimeType(displayName: unknown, uri: string): string {
  const source =
    typeof displayName === "string" && displayName.trim() ? displayName : uri
  const extension = source.split(/[?#]/, 1)[0].split(".").pop()?.toLowerCase()
  switch (extension) {
    case "jpg":
    case "jpeg":
      return "image/jpeg"
    case "webp":
      return "image/webp"
    case "gif":
      return "image/gif"
    case "avif":
      return "image/avif"
    case "svg":
    case "svg+xml":
      return "image/svg+xml"
    default:
      return "image/png"
  }
}
