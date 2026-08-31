const MEMORY_POLICY_REQUIRED_CODE = "memory_policy_required"

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function containsMemoryPolicyError(
  value: unknown,
  seen = new Set<object>()
): boolean {
  if (typeof value === "string") {
    try {
      return containsMemoryPolicyError(JSON.parse(value), seen)
    } catch {
      return false
    }
  }
  if (Array.isArray(value))
    return value.some((item) => containsMemoryPolicyError(item, seen))

  const record = asRecord(value)
  if (!record) return false
  if (seen.has(record)) return false
  seen.add(record)
  if (record.code === MEMORY_POLICY_REQUIRED_CODE) return true

  return Object.values(record).some((item) =>
    containsMemoryPolicyError(item, seen)
  )
}

/**
 * Identify the expected gateway response when a memory call skipped the
 * current-turn policy preflight. It is a retryable protocol condition, not a
 * user-facing tool failure, so callers can keep the tool quiet without
 * changing the gateway's `isError` contract.
 */
export function isMemoryPolicyRequiredError(value: unknown): boolean {
  if (containsMemoryPolicyError(value)) return true
  if (typeof value !== "string") return false

  try {
    if (containsMemoryPolicyError(JSON.parse(value))) return true
  } catch {
    // Some hosts prepend labels or an execution wrapper to the JSON payload.
  }

  return (
    /(?:^|["'\s])code["']?\s*[:\n]\s*["']?memory_policy_required/i.test(
      value
    ) &&
    /(?:^|["'\s])(?:memoryPolicyRequired|reason)["']?\s*[:\n]\s*(?:true|["']?memory_policy_not_loaded_for_current_turn)/i.test(
      value
    )
  )
}

export function normalizeToolResultError(
  output: string | null | undefined,
  isError: boolean,
  detectionValue: unknown = output
): { output: string | null; isError: boolean } {
  if (
    isMemoryPolicyRequiredError(detectionValue) ||
    isMemoryPolicyRequiredError(output)
  ) {
    return { output: null, isError: false }
  }
  return { output: output ?? null, isError }
}
