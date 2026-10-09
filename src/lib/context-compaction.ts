/**
 * ACP context-compaction supports a legacy boolean marker and a versioned
 * payload. Both must remain recognizable for historical conversations.
 */
export function isContextCompactionMeta(meta: unknown): boolean {
  if (!meta || typeof meta !== "object") return false

  const marker = (meta as Record<string, unknown>).contextCompaction
  if (marker === true) return true
  if (!marker || typeof marker !== "object") return false

  const version = (marker as Record<string, unknown>).version
  return (
    typeof version === "number" && Number.isInteger(version) && version >= 1
  )
}

/** Return the versioned payload; the legacy boolean marker has no body. */
export function contextCompactionPayload(
  meta: unknown
): Record<string, unknown> | null {
  if (!isContextCompactionMeta(meta)) return null

  const marker = (meta as Record<string, unknown>).contextCompaction
  return marker && typeof marker === "object"
    ? (marker as Record<string, unknown>)
    : null
}

/** 只有后端明确确认的保留摘要才可展示，兼容上游的历史标记。 */
export function contextCompactionSummary(
  meta: unknown,
  output: string | null | undefined
): string | null {
  if (!isContextCompactionMeta(meta)) return null
  const record = meta as Record<string, unknown>
  if (
    record["iyw.compactionSummary"] !== true &&
    record["codeg.compactionSummary"] !== true
  )
    return null
  return typeof output === "string" && output.trim() ? output : null
}
