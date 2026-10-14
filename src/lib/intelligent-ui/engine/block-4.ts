// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { type UIValue, ExpressionError } from "./expression"
import { ResolvedAction } from "./block-3"
import { BLOCK_LIMITS } from "./block-1"

// Reads a button's action from its resolved props. `set` values were already
// evaluated as templates against the state at the time of this resolution.
export function readAction(
  props: Readonly<Record<string, unknown>>,
  stateNames: ReadonlySet<string>
): ResolvedAction | undefined {
  if (typeof props.ask === "string" && props.ask.trim() !== "") {
    return {
      kind: "ask",
      text: props.ask.trim().slice(0, BLOCK_LIMITS.askLength),
    }
  }
  if (typeof props.copy === "string" && props.copy !== "") {
    return { kind: "copy", text: props.copy }
  }
  if (isObject(props.set)) {
    const patch: Record<string, UIValue> = {}
    for (const [name, value] of Object.entries(props.set)) {
      const json = toUIValue(value, 1)
      if (stateNames.has(name) && json !== undefined) {
        setOwn(patch, name, json)
      }
    }
    return Object.keys(patch).length > 0 ? { kind: "set", patch } : undefined
  }
  return undefined
}

// ---- Helpers ----
export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

// Keys come from model output: a plain `target[key] = value` with "__proto__"
// would replace the object's prototype instead of adding a field.
export function setOwn<T>(
  target: Record<string, T>,
  key: string,
  value: T
): void {
  Object.defineProperty(target, key, {
    value,
    enumerable: true,
    writable: true,
    configurable: true,
  })
}

export function isIdentifier(name: string): boolean {
  return (
    /^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(name) &&
    !["__proto__", "prototype", "constructor"].includes(name) &&
    !name.startsWith("__view_")
  )
}

// A JSON value within the state depth limit, or undefined.
export function asUIValue(value: unknown): UIValue | undefined {
  return toUIValue(value, 1)
}

export function toUIValue(value: unknown, depth: number): UIValue | undefined {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "boolean"
  ) {
    return value
  }
  if (typeof value === "number") {
    return Number.isFinite(value) ? value : undefined
  }
  if (depth > BLOCK_LIMITS.stateDepth) {
    return undefined
  }
  if (Array.isArray(value)) {
    const items: UIValue[] = []
    for (const item of value) {
      const json = toUIValue(item, depth + 1)
      if (json === undefined) {
        return undefined
      }
      items.push(json)
    }
    return items
  }
  if (isObject(value)) {
    const result: Record<string, UIValue> = {}
    for (const [key, field] of Object.entries(value)) {
      const json = toUIValue(field, depth + 1)
      if (json === undefined) {
        return undefined
      }
      setOwn(result, key, json)
    }
    return result
  }
  return undefined
}

export function errorMessage(error: unknown): string {
  if (error instanceof ExpressionError || error instanceof Error) {
    return error.message
  }
  return String(error)
}
