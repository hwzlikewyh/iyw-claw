// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { UIValue, ExpressionError, MAX_RANGE_POINTS } from "./expression-1"
import { ExpressionBudget, spend } from "./expression-2"
import { toNumber, stringify, valuesEqual } from "./expression-3"

export type Fn = (args: UIValue[], budget: ExpressionBudget) => UIValue

export function numbers(
  name: string,
  args: UIValue[],
  range: number | readonly [number, number]
): number[] {
  const [min, max] = typeof range === "number" ? [range, range] : range
  if (args.length < min || args.length > max) {
    throw new ExpressionError(
      `${name} takes ${min === max ? min : `${min}-${max}`} arguments`
    )
  }
  return args.map((arg) => toNumber(arg, name))
}

export function math(name: string, fn: (value: number) => number): Fn {
  return (args) => fn(numbers(name, args, 1)[0] ?? Number.NaN)
}

export function list(
  name: string,
  value: UIValue | undefined,
  budget: ExpressionBudget
): number[] {
  if (!Array.isArray(value)) {
    throw new ExpressionError(`${name} expects a list`)
  }
  spend(budget, value.length)
  return value.map((item) => toNumber(item, name))
}

export function extreme(name: string, pick: (values: number[]) => number): Fn {
  return (args, budget) => {
    const values =
      args.length === 1 && Array.isArray(args[0])
        ? list(name, args[0], budget)
        : numbers(name, args, [1, 64])
    if (values.length === 0) {
      throw new ExpressionError(`${name} of an empty list`)
    }
    return pick(values)
  }
}

export const FUNCTIONS: Record<string, Fn> = {
  abs: math("abs", Math.abs),
  sqrt: math("sqrt", Math.sqrt),
  cbrt: math("cbrt", Math.cbrt),
  floor: math("floor", Math.floor),
  ceil: math("ceil", Math.ceil),
  sign: math("sign", Math.sign),
  log: math("log", Math.log),
  ln: math("ln", Math.log),
  log10: math("log10", Math.log10),
  log2: math("log2", Math.log2),
  exp: math("exp", Math.exp),
  sin: math("sin", Math.sin),
  cos: math("cos", Math.cos),
  tan: math("tan", Math.tan),
  asin: math("asin", Math.asin),
  acos: math("acos", Math.acos),
  atan: math("atan", Math.atan),
  sinh: math("sinh", Math.sinh),
  cosh: math("cosh", Math.cosh),
  tanh: math("tanh", Math.tanh),
  pow: (args) => {
    const [base = Number.NaN, exponent = Number.NaN] = numbers("pow", args, 2)
    return base ** exponent
  },
  min: extreme("min", (values) => Math.min(...values)),
  max: extreme("max", (values) => Math.max(...values)),
  round: (args) => {
    const [value = Number.NaN, digits = 0] = numbers("round", args, [1, 2])
    const factor = 10 ** Math.trunc(digits)
    return Math.round(value * factor) / factor
  },
  fixed: (args) => {
    const [value = Number.NaN, digits = 0] = numbers("fixed", args, [1, 2])
    return Number.isFinite(value)
      ? value.toFixed(Math.min(10, Math.max(0, Math.trunc(digits))))
      : "—"
  },
  clamp: (args) => {
    const [value = Number.NaN, low = Number.NaN, high = Number.NaN] = numbers(
      "clamp",
      args,
      3
    )
    return Math.min(high, Math.max(low, value))
  },
  // range(start, end, step = ±1), both ends included: range(1, years) is every year.
  range: (args, budget) => {
    const [start = 0, end = 0, given] = numbers("range", args, [2, 3])
    const step = given ?? (end >= start ? 1 : -1)
    if (
      !Number.isFinite(start) ||
      !Number.isFinite(end) ||
      !Number.isFinite(step) ||
      step === 0 ||
      (end - start) / step < 0
    ) {
      throw new ExpressionError(
        "range needs a step that moves from start to end"
      )
    }
    const count = Math.floor((end - start) / step + 1e-9) + 1
    if (count > MAX_RANGE_POINTS) {
      throw new ExpressionError(
        `range is limited to ${MAX_RANGE_POINTS} points`
      )
    }
    spend(budget, count)
    return Array.from({ length: count }, (_, index) =>
      roundFloat(start + index * step)
    )
  },
  sum: (args, budget) =>
    list("sum", args[0], budget).reduce((total, value) => total + value, 0),
  avg: (args, budget) => {
    const values = list("avg", args[0], budget)
    if (values.length === 0) {
      throw new ExpressionError("avg of an empty list")
    }
    return values.reduce((total, value) => total + value, 0) / values.length
  },
  len: (args) => {
    const [value] = args
    if (Array.isArray(value) || typeof value === "string") {
      return value.length
    }
    throw new ExpressionError("len expects a list or text")
  },
  contains: (args, budget) => {
    const [container, needle = null] = args
    if (typeof container === "string") {
      return container.includes(stringify(needle))
    }
    if (Array.isArray(container)) {
      spend(budget, container.length)
      return container.some((item) => valuesEqual(item, needle))
    }
    throw new ExpressionError("contains expects a list or text")
  },
}

export function roundFloat(value: number): number {
  return Math.round(value * 1e10) / 1e10
}

export const EXPRESSION_FUNCTIONS: readonly string[] = Object.keys(FUNCTIONS)
