function object(value: unknown): Record<string, unknown> | null {
  if (value && typeof value === "object" && !Array.isArray(value))
    return value as Record<string, unknown>
  if (typeof value !== "string") return null
  try {
    const parsed: unknown = JSON.parse(value)
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null
  } catch {
    return null
  }
}

export function permissionSummary(toolCall: unknown) {
  const call = object(toolCall)
  const input = object(
    call?.rawInput ?? call?.raw_input ?? call?.input ?? call?.arguments
  )
  for (const source of [input, call]) {
    for (const key of ["justification", "description", "reason"]) {
      const value = source?.[key]
      if (typeof value === "string" && value.trim()) return value.trim()
    }
  }
  return null
}
