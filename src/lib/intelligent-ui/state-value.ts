import type { UIValue } from "./engine/expression"
const MAX_BYTES = 4096
const MAX_DEPTH = 4
const NAME = /^(?!__proto__$|prototype$|constructor$)[A-Za-z0-9_-]{1,64}$/

export function validState(value: unknown): value is Record<string, UIValue> {
  try {
    return (
      Boolean(value) &&
      typeof value === "object" &&
      !Array.isArray(value) &&
      validValue(value, 0) &&
      new TextEncoder().encode(JSON.stringify(value)).length <= MAX_BYTES
    )
  } catch {
    return false
  }
}
function validValue(value: unknown, depth: number): boolean {
  if (depth > MAX_DEPTH) return false
  if (value === null || typeof value === "boolean" || typeof value === "string")
    return true
  if (typeof value === "number") return Number.isFinite(value)
  if (Array.isArray(value))
    return value.every((item) => validValue(item, depth + 1))
  return (
    typeof value === "object" &&
    value !== null &&
    Object.entries(value).every(
      ([name, item]) => NAME.test(name) && validValue(item, depth + 1)
    )
  )
}
