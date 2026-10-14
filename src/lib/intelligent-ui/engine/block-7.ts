// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { type UIValue, valuesEqual } from "./expression"
import { hasOwn } from "../value"
import { toUIValue, setOwn } from "./block-4"

export function mergeState(
  initial: Readonly<Record<string, UIValue>>,
  saved: Readonly<Record<string, unknown>> | undefined
): Record<string, UIValue> {
  const result: Record<string, UIValue> = { ...initial }
  if (!saved) {
    return result
  }
  for (const [name, value] of Object.entries(saved)) {
    if (!hasOwn(initial, name)) {
      continue
    }
    const json = toUIValue(value, 1)
    const declared = initial[name] ?? null
    if (json !== undefined && sameShape(declared, json)) {
      setOwn(result, name, json)
    }
  }
  return result
}

export function stateDiff(
  initial: Readonly<Record<string, UIValue>>,
  state: Readonly<Record<string, UIValue>>
): Record<string, UIValue> | undefined {
  const diff: Record<string, UIValue> = {}
  for (const [name, value] of Object.entries(state)) {
    if (hasOwn(initial, name) && !valuesEqual(value, initial[name] ?? null)) {
      setOwn(diff, name, value)
    }
  }
  return Object.keys(diff).length > 0 ? diff : undefined
}

export function sameShape(declared: UIValue, value: UIValue): boolean {
  if (declared === null || value === null) {
    return true
  }
  if (Array.isArray(declared)) {
    return Array.isArray(value)
  }
  return (
    typeof declared === typeof value &&
    Array.isArray(declared) === Array.isArray(value)
  )
}
