type ArtifactRecord = Record<string, unknown>

export function extractAcceptedPaths(output: string | null | undefined) {
  const accepted = acceptedArtifacts(output)
  return (
    accepted?.flatMap((item) =>
      typeof item.path === "string" ? [item.path] : []
    ) ?? null
  )
}

export function extractAcceptedIds(output: string | null | undefined) {
  return (
    acceptedArtifacts(output)?.flatMap((item) =>
      typeof item.id === "number" && Number.isInteger(item.id) && item.id > 0
        ? [item.id]
        : []
    ) ?? []
  )
}

function acceptedArtifacts(output: string | null | undefined) {
  if (output && indicatesRejectedArtifactCall(output)) return []
  const accepted = resultLayers(output)
    .map((layer) => layer?.accepted)
    .find((value) => value != null)
  if (!Array.isArray(accepted)) return null
  return accepted.flatMap((value) => {
    const record = parseNestedRecord(value)
    return record ? [record] : []
  })
}

export function extractMessageId(output: string | null | undefined) {
  return (
    resultLayers(output)
      .flatMap((layer) => [layer?.message_id, layer?.messageId])
      .find(
        (value): value is string =>
          typeof value === "string" && value.trim() !== ""
      ) ?? null
  )
}

function resultLayers(output: string | null | undefined) {
  const parsed = parseRecord(output)
  const result = parseNestedRecord(parsed?.result)
  return [
    parseNestedRecord(parsed?.structuredContent ?? parsed?.structured_content),
    parsed,
    parseNestedRecord(result?.structuredContent ?? result?.structured_content),
    result,
    extractDeliveryArtifact(output),
  ]
}

export function extractDeliveryArtifact(output: string | null | undefined) {
  const parsed = parseRecord(output)
  const result = parseNestedRecord(parsed?.result)
  const structured = parseNestedRecord(
    parsed?.structuredContent ?? parsed?.structured_content
  )
  const resultStructured = parseNestedRecord(
    result?.structuredContent ?? result?.structured_content
  )
  const layers = [structured, resultStructured, result, parsed]
  return (
    layers
      .map((layer) =>
        parseNestedRecord(parseNestedRecord(layer?.delivery)?.artifact)
      )
      .find((artifact) => artifact !== null) ?? null
  )
}

export function indicatesRejectedArtifactCall(output: string): boolean {
  if (/Task artifact registration failed:/i.test(output)) return true
  return /Presented\s+0\s+task artifact\(s\)/i.test(output)
}

export function parseRecord(value: string | null | undefined) {
  if (!value) return null
  const direct = parseJsonRecord(value)
  if (direct) return direct
  const lines = value.split(/\r?\n/)
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    const parsed = parseJsonRecord(lines[index].trim())
    if (parsed) return parsed
  }
  return null
}

function parseJsonRecord(value: string) {
  try {
    return parseNestedRecord(JSON.parse(value))
  } catch {
    return null
  }
}

export function parseNestedRecord(value: unknown): ArtifactRecord | null {
  if (typeof value === "string") {
    try {
      return parseNestedRecord(JSON.parse(value))
    } catch {
      return null
    }
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) return null
  return value as ArtifactRecord
}
