// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。

// Expression language for model-written formulas in interactive blocks.
//
// Whitelisted syntax, parsed to an AST and interpreted; never touches `eval` or
// `Function`. Values are JSON values. Grammar, loosest to tightest:
//
//   cond ? a : b      ||      &&      == !=      < <= > >=      + -      * / %
//   unary ! -         ^ (right-associative; -2^2 is -(2^2))      a.b  a[i]  f(x)
//
// Literals: numbers, 'strings' or "strings", true / false / null, [a, b].
// `+` concatenates when either side is a string. Member access reads own
// properties only and rejects __proto__ / constructor / prototype.
//
// Every evaluation draws from a step budget, so `range`, `sum` and nested
// formulas cannot run away. There is no assignment, loop or user function:
// imperative logic belongs in an Artifact.

export type UIValue =
  | number
  | string
  | boolean
  | null
  | UIValue[]
  | { [key: string]: UIValue }

export class ExpressionError extends Error {
  override name = "ExpressionError"
}

export const MAX_EXPRESSION_LENGTH = 512

export const MAX_RANGE_POINTS = 1000

// Default step budget for one standalone evaluation. Block resolution shares one
// larger budget across all of its formulas instead.
export const DEFAULT_STEPS = 100_000

export const FORBIDDEN_MEMBERS = new Set([
  "__proto__",
  "constructor",
  "prototype",
])

export const KEYWORDS: Record<string, UIValue> = {
  true: true,
  false: false,
  null: null,
}

export const CONSTANTS: Record<string, number> = { pi: Math.PI, e: Math.E }

export type Token =
  | { kind: "number"; value: number }
  | { kind: "string"; value: string }
  | { kind: "ident"; value: string }
  | { kind: "op"; value: string }
  | { kind: "punct"; value: "(" | ")" | "[" | "]" | "," | "." | "?" | ":" }
  | { kind: "end" }

export type Node =
  | { kind: "literal"; value: UIValue }
  | { kind: "array"; items: Node[] }
  | { kind: "var"; name: string }
  | { kind: "unary"; op: "-" | "!"; operand: Node }
  | { kind: "binary"; op: string; left: Node; right: Node }
  | { kind: "logical"; op: "&&" | "||"; left: Node; right: Node }
  | { kind: "conditional"; test: Node; then: Node; otherwise: Node }
  | { kind: "member"; object: Node; property: Node; computed: boolean }
  | { kind: "call"; name: string; args: Node[] }

export const BINARY_PRECEDENCE: Record<string, number> = {
  "||": 1,
  "&&": 2,
  "==": 3,
  "!=": 3,
  "<": 4,
  "<=": 4,
  ">": 4,
  ">=": 4,
  "+": 5,
  "-": 5,
  "*": 6,
  "/": 6,
  "%": 6,
  "^": 8,
}

export const POWER_PRECEDENCE = 8

// Operators models write out of JavaScript habit are read as their equivalents.
export const OPERATOR_ALIASES: Record<string, string> = {
  "===": "==",
  "!==": "!=",
}

export const OPERATORS = [
  "===",
  "!==",
  "==",
  "!=",
  "<=",
  ">=",
  "&&",
  "||",
  "<",
  ">",
  "+",
  "-",
  "*",
  "/",
  "%",
  "^",
  "!",
]

export function tokenize(source: string): Token[] {
  if (source.length > MAX_EXPRESSION_LENGTH)
    throw new ExpressionError("expression too long")
  const tokens: Token[] = []
  let index = 0
  while (index < source.length) {
    if (/\s/.test(source[index])) {
      index++
      continue
    }
    const next = readToken({
      source,
      index,
      previous: tokens[tokens.length - 1],
    })
    tokens.push(next.token)
    index = next.end
  }
  tokens.push({ kind: "end" })
  return tokens
}
function readToken({
  source,
  index,
  previous,
}: {
  source: string
  index: number
  previous?: Token
}): { token: Token; end: number } {
  const char = source[index] ?? ""
  const dotIsMember =
    char === "." && previous !== undefined && isValueEnd(previous)
  if (
    /[0-9]/.test(char) ||
    (char === "." && !dotIsMember && /[0-9]/.test(source[index + 1] ?? ""))
  ) {
    const match = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(source.slice(index))
    if (!match) throw new ExpressionError(`bad number at ${index}`)
    return {
      token: { kind: "number", value: Number(match[0]) },
      end: index + match[0].length,
    }
  }
  if (/[a-zA-Z_]/.test(char)) {
    const value =
      /^[a-zA-Z_][a-zA-Z0-9_]*/.exec(source.slice(index))?.[0] ?? char
    return { token: { kind: "ident", value }, end: index + value.length }
  }
  if (char === "'" || char === '"') {
    const { value, end } = readString(source, index)
    return { token: { kind: "string", value }, end }
  }
  if ("()[],.?:".includes(char))
    return {
      token: {
        kind: "punct",
        value: char as "(" | ")" | "[" | "]" | "," | "." | "?" | ":",
      },
      end: index + 1,
    }
  const operator = OPERATORS.find((candidate) =>
    source.startsWith(candidate, index)
  )
  if (operator)
    return {
      token: { kind: "op", value: OPERATOR_ALIASES[operator] ?? operator },
      end: index + operator.length,
    }
  throw new ExpressionError(`unexpected "${char}" at ${index}`)
}

export function isValueEnd(token: Token): boolean {
  return (
    token.kind === "ident" ||
    token.kind === "number" ||
    token.kind === "string" ||
    (token.kind === "punct" && (token.value === ")" || token.value === "]"))
  )
}

export function readString(
  source: string,
  start: number
): { value: string; end: number } {
  const quote = source[start]
  let value = ""
  let index = start + 1
  while (index < source.length) {
    const char = source[index] ?? ""
    if (char === "\\") {
      const next = source[index + 1] ?? ""
      value += next === "n" ? "\n" : next === "t" ? "\t" : next
      index += 2
      continue
    }
    if (char === quote) {
      return { value, end: index + 1 }
    }
    value += char
    index += 1
  }
  throw new ExpressionError("unterminated string")
}
