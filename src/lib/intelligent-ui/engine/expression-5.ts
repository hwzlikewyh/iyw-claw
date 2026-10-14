import { hasOwn } from "../value"
// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import {
  Node,
  ExpressionError,
  BINARY_PRECEDENCE,
  tokenize,
  UIValue,
} from "./expression-1"
import {
  LatexValues,
  wrapLatex,
  formatLatexNumber,
  LATEX_CONSTANTS,
  variableLatex,
  LATEX_FUNCTIONS,
  mayBeNumber,
  collectVariables,
  parseExpression,
  scopeFromRecord,
} from "./expression-4"
import { Parser } from "./expression-2"

export function nodeToLatex(node: Node, values?: LatexValues): string {
  const wrap = (child: Node, minPrecedence: number) =>
    wrapLatex(child, minPrecedence, values)
  switch (node.kind) {
    case "literal":
      if (typeof node.value !== "number") {
        throw new ExpressionError("only numeric formulas render as math")
      }
      return formatLatexNumber(node.value)
    case "var":
      if (values && hasOwn(values, node.name)) {
        return `(${formatLatexNumber(values[node.name] ?? Number.NaN)})`
      }
      return LATEX_CONSTANTS[node.name] ?? variableLatex(node.name)
    case "unary":
      if (node.op !== "-") {
        throw new ExpressionError("only numeric formulas render as math")
      }
      return `-${wrap(node.operand, 6)}`
    case "binary":
      return binaryToLatex(node, values)

    case "call":
      return callToLatex(node, values)

    default:
      throw new ExpressionError("only numeric formulas render as math")
  }
}

// LaTeX for display next to an editable formula. Throws ExpressionError when the
// formula is invalid or not purely numeric.
export function expressionToLatex(source: string): string {
  return nodeToLatex(new Parser(tokenize(source)).parse())
}

// ---- Worked formulas (formula primitive) ----

export type FormulaLatex = {
  // name = expression, e.g. F_{g} = m g
  symbolic: string
  // The line under it: = (9) (9.8) = 88.2\,\mathrm{N}. Absent when a value is
  // missing or not a number.
  worked?: string
  result?: number
}

// LaTeX for a formula shown with its current values: `name = expr`, then the
// line `= (values) = result unit`. Built from the parsed formula only, with the
// unit in \text and LaTeX specials escaped, so no model-written LaTeX reaches
// the renderer.
// `with` gives the value substituted for a name when it differs from the block's
// (a slider in 10^24 kg, a formula in kg): a number, or a formula of the block's
// values written with or without braces, as in `computed`.
export function formulaToLatex(
  source: string,
  options: {
    name?: string
    unit?: string
    values?: Readonly<Record<string, UIValue>>
    with?: unknown
    precision?: number
  } = {}
): FormulaLatex {
  const ast = new Parser(tokenize(source)).parse()
  if (!mayBeNumber(ast)) {
    throw new ExpressionError("expected a numeric formula")
  }
  const name =
    options.name && /^[A-Za-z_][A-Za-z0-9_]{0,31}$/.test(options.name)
      ? `${variableLatex(options.name)} = `
      : ""
  const symbolic = `${name}${nodeToLatex(ast)}`
  const numbers = formulaNumbers(ast, options)
  if (!numbers) return { symbolic }
  let result: number
  try {
    const value = parseExpression(source).evaluate(scopeFromRecord(numbers))
    if (typeof value !== "number" || !Number.isFinite(value)) {
      return { symbolic }
    }
    result = value
  } catch {
    return { symbolic }
  }
  const digits = Math.min(6, Math.max(0, Math.round(options.precision ?? 2)))
  const rounded = Number(result.toFixed(digits))
  const unit = options.unit?.trim()
    ? `\\,${unitLatex(options.unit.trim().slice(0, 24))}`
    : ""
  // The worked line continues the formula above it instead of repeating it.
  const steps =
    Object.keys(numbers).length > 0 ? `= ${nodeToLatex(ast, numbers)} ` : ""
  return {
    symbolic,
    worked: `${steps}= ${formatLatexNumber(rounded)}${unit}`,
    result,
  }
}

export function overrideValue(
  value: unknown,
  values: Readonly<Record<string, UIValue>>
): UIValue | undefined {
  if (typeof value === "number") {
    return value
  }
  if (typeof value !== "string") {
    return undefined
  }
  const trimmed = value.trim()
  const source =
    trimmed.startsWith("{") && trimmed.endsWith("}")
      ? trimmed.slice(1, -1)
      : trimmed
  try {
    return parseExpression(source).evaluate(scopeFromRecord(values))
  } catch {
    return undefined
  }
}

// Plain unit strings (`N`, `m/s^2`, `kg*m/s^-1`) read as upright math; anything
// else (`元/月`, `万元`) is shown as text.
export function unitLatex(unit: string): string {
  if (/^[A-Za-z]+(\^-?\d)?([*/][A-Za-z]+(\^-?\d)?)*$/.test(unit)) {
    return unit
      .replace(
        /([A-Za-z]+)(?:\^(-?\d))?/g,
        (_, word: string, power?: string) =>
          `\\mathrm{${word}}${power ? `^{${power}}` : ""}`
      )
      .replace(/\*/g, "\\cdot ")
  }
  return `\\text{${escapeLatexText(unit)}}`
}

export function escapeLatexText(text: string): string {
  return text.replace(/[\\{}$&#^_%~]/g, (char) => {
    switch (char) {
      case "\\":
        return "\\textbackslash{}"
      case "^":
        return "\\textasciicircum{}"
      case "~":
        return "\\textasciitilde{}"
      default:
        return `\\${char}`
    }
  })
}

function binaryToLatex(
  node: Extract<Node, { kind: "binary" }>,
  values?: LatexValues
): string {
  const wrap = (child: Node, minPrecedence: number) =>
    wrapLatex(child, minPrecedence, values)
  const latex = (child: Node) => nodeToLatex(child, values)
  const precedence = BINARY_PRECEDENCE[node.op] ?? 0
  switch (node.op) {
    case "/":
      return `\\frac{${latex(node.left)}}{${latex(node.right)}}`
    case "^":
      return `{${wrap(node.left, 10)}}^{${latex(node.right)}}`
    case "*": {
      // Juxtapose when the right factor cannot be misread as part of a number.
      const right = wrap(node.right, precedence)
      const substituted =
        node.right.kind === "var" &&
        values !== undefined &&
        hasOwn(values, node.right.name)
      const glue =
        (node.right.kind === "literal" || node.right.kind === "unary") &&
        !substituted
          ? " \\cdot "
          : " "
      return `${wrap(node.left, precedence)}${glue}${right}`
    }
    case "%":
      return `${wrap(node.left, precedence)} \\bmod ${wrap(node.right, precedence + 1)}`
    case "+":
    case "-":
      return `${wrap(node.left, precedence)} ${node.op} ${wrap(node.right, precedence + 1)}`
    default:
      throw new ExpressionError("only numeric formulas render as math")
  }
}

function callToLatex(
  node: Extract<Node, { kind: "call" }>,
  values?: LatexValues
): string {
  const wrap = (child: Node, minPrecedence: number) =>
    wrapLatex(child, minPrecedence, values)
  const latex = (child: Node) => nodeToLatex(child, values)
  const args = node.args.map(latex)
  switch (node.name) {
    case "sqrt":
      return `\\sqrt{${args[0] ?? ""}}`
    case "cbrt":
      return `\\sqrt[3]{${args[0] ?? ""}}`
    case "abs":
      return `\\left|${args[0] ?? ""}\\right|`
    case "floor":
      return `\\lfloor ${args[0] ?? ""} \\rfloor`
    case "ceil":
      return `\\lceil ${args[0] ?? ""} \\rceil`
    case "pow":
      return `{${wrap(node.args[0] ?? { kind: "literal", value: 0 }, 10)}}^{${args[1] ?? ""}}`
    default: {
      const name = LATEX_FUNCTIONS[node.name] ?? `\\operatorname{${node.name}}`
      return `${name}\\left(${args.join(", ")}\\right)`
    }
  }
}

function formulaNumbers(
  ast: Node,
  options: Parameters<typeof formulaToLatex>[1]
) {
  const names = new Set<string>()
  collectVariables(ast, names)
  const numbers: Record<string, number> = {}
  const overrides =
    options?.with !== null &&
    typeof options?.with === "object" &&
    !Array.isArray(options?.with)
      ? options?.with
      : {}
  for (const variable of names) {
    const value = hasOwn(overrides, variable)
      ? overrideValue(Reflect.get(overrides, variable), options?.values ?? {})
      : options?.values?.[variable]
    if (typeof value !== "number" || !Number.isFinite(value)) {
      return undefined
    }
    Object.defineProperty(numbers, variable, { value, enumerable: true })
  }
  return numbers
}
