// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { UIValue, ExpressionError, FORBIDDEN_MEMBERS } from "./expression-1"
import { hasOwn } from "../value"
import { ExpressionScope } from "./expression-2"

export function evaluateBinary(
  op: string,
  left: UIValue,
  right: UIValue
): UIValue {
  switch (op) {
    case "+":
      if (typeof left === "string" || typeof right === "string") {
        return stringify(left) + stringify(right)
      }
      return toNumber(left, op) + toNumber(right, op)
    case "-":
      return toNumber(left, op) - toNumber(right, op)
    case "*":
      return toNumber(left, op) * toNumber(right, op)
    case "/":
      return toNumber(left, op) / toNumber(right, op)
    case "%":
      return toNumber(left, op) % toNumber(right, op)
    case "^":
      return toNumber(left, op) ** toNumber(right, op)
    case "==":
      return valuesEqual(left, right)
    case "!=":
      return !valuesEqual(left, right)
    case "<":
    case "<=":
    case ">":
    case ">=":
      return compare(op, left, right)
    default:
      throw new ExpressionError(`unknown operator ${op}`)
  }
}

export function compare(op: string, left: UIValue, right: UIValue): boolean {
  if (typeof left === "string" && typeof right === "string") {
    return op === "<"
      ? left < right
      : op === "<="
        ? left <= right
        : op === ">"
          ? left > right
          : left >= right
  }
  const a = toNumber(left, op)
  const b = toNumber(right, op)
  return op === "<" ? a < b : op === "<=" ? a <= b : op === ">" ? a > b : a >= b
}

export function readMember(object: UIValue, property: UIValue): UIValue {
  if (typeof property === "string" && FORBIDDEN_MEMBERS.has(property)) {
    throw new ExpressionError(`property ${property} is not allowed`)
  }
  if (Array.isArray(object)) {
    if (property === "length") {
      return object.length
    }
    if (
      typeof property === "number" &&
      Number.isInteger(property) &&
      property >= 0 &&
      property < object.length
    ) {
      return object[property] ?? null
    }
    throw new ExpressionError("index out of range")
  }
  if (typeof object === "string") {
    if (property === "length") {
      return object.length
    }
    throw new ExpressionError("strings only have length")
  }
  if (
    object !== null &&
    typeof object === "object" &&
    typeof property === "string"
  ) {
    if (!hasOwn(object, property)) {
      throw new ExpressionError(`unknown property ${property}`)
    }
    return object[property] ?? null
  }
  throw new ExpressionError("cannot read a property here")
}

export function truthy(value: UIValue): boolean {
  if (value === null || value === false || value === 0 || value === "") {
    return false
  }
  return !(typeof value === "number" && Number.isNaN(value))
}

export function toNumber(value: UIValue, context: string): number {
  if (typeof value === "number") {
    return value
  }
  throw new ExpressionError(`${context} expects numbers`)
}

export function stringify(value: UIValue): string {
  if (typeof value === "string") {
    return value
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value)
  }
  if (value === null) {
    return ""
  }
  return JSON.stringify(value)
}

export function valuesEqual(left: UIValue, right: UIValue): boolean {
  if (left === right) {
    return true
  }
  if (Array.isArray(left) && Array.isArray(right)) {
    return (
      left.length === right.length &&
      left.every((item, index) => valuesEqual(item, right[index] ?? null))
    )
  }
  if (isPlainValueObject(left) && isPlainValueObject(right)) {
    const keys = Object.keys(left)
    return (
      keys.length === Object.keys(right).length &&
      keys.every(
        (key) =>
          hasOwn(right, key) &&
          valuesEqual(left[key] ?? null, right[key] ?? null)
      )
    )
  }
  return false
}

export function isPlainValueObject(
  value: UIValue
): value is { [key: string]: UIValue } {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

export type Expression = {
  readonly source: string
  // Names the expression reads from its scope (constants and functions excluded).
  readonly variables: readonly string[]
  evaluate: (scope: ExpressionScope) => UIValue
}

export {
  type Fn,
  numbers,
  math,
  list,
  extreme,
  FUNCTIONS,
  roundFloat,
  EXPRESSION_FUNCTIONS,
} from "./expression-6"
