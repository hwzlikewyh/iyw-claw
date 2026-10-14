import { hasOwn } from "../value"
// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { Expression } from "./expression-3"
import {
  Parser,
  evaluateNode,
  createBudget,
  ExpressionScope,
} from "./expression-2"
import {
  tokenize,
  Node,
  CONSTANTS,
  UIValue,
  ExpressionError,
  BINARY_PRECEDENCE,
} from "./expression-1"
import { nodeToLatex } from "./expression-5"

export function parseExpression(source: string): Expression {
  const ast = new Parser(tokenize(source)).parse()
  const names = new Set<string>()
  collectVariables(ast, names)
  return {
    source,
    variables: Array.from(names),
    evaluate: (scope) => evaluateNode(ast, scope),
  }
}

export function collectVariables(node: Node, names: Set<string>): void {
  switch (node.kind) {
    case "var":
      if (!hasOwn(CONSTANTS, node.name)) {
        names.add(node.name)
      }
      return
    case "array":
      for (const item of node.items) {
        collectVariables(item, names)
      }
      return
    case "unary":
      collectVariables(node.operand, names)
      return
    case "binary":
    case "logical":
      collectVariables(node.left, names)
      collectVariables(node.right, names)
      return
    case "conditional":
      collectVariables(node.test, names)
      collectVariables(node.then, names)
      collectVariables(node.otherwise, names)
      return
    case "member":
      collectVariables(node.object, names)
      if (node.computed) {
        collectVariables(node.property, names)
      }
      return
    case "call":
      for (const arg of node.args) {
        collectVariables(arg, names)
      }
      return
    default:
      return
  }
}

export function scopeFromRecord(
  values: Readonly<Record<string, UIValue>>,
  budget = createBudget()
): ExpressionScope {
  return {
    lookup: (name) => (hasOwn(values, name) ? values[name] : undefined),
    budget,
  }
}

// ---- Numeric formulas (calculator, function-plot) ----

export type CompiledExpression = {
  evaluate: (variables: Readonly<Record<string, number>>) => number
  variables: readonly string[]
}

// A formula whose result must be a number. Errors: "unknown function <name>" at
// compile time, "unknown variable <name>" at evaluation; callers surface both.
export function compileExpression(source: string): CompiledExpression {
  const expression = parseExpression(source)
  // A comparison, logic or text at the top can never be a number: say so now
  // rather than failing at every point.
  const root = new Parser(tokenize(source)).parse()
  if (!mayBeNumber(root)) {
    throw new ExpressionError("expected a numeric formula")
  }
  return {
    variables: expression.variables,
    evaluate: (variables) => {
      const value = expression.evaluate(scopeFromRecord(variables))
      if (typeof value !== "number") {
        throw new ExpressionError("expected a number")
      }
      return value
    },
  }
}

export const NON_NUMERIC_OPERATORS = new Set(["==", "!=", "<", "<=", ">", ">="])

export function mayBeNumber(node: Node): boolean {
  switch (node.kind) {
    case "literal":
      return typeof node.value === "number"
    case "array":
      return false
    case "unary":
      return node.op === "-"
    case "binary":
      return !NON_NUMERIC_OPERATORS.has(node.op)
    case "logical":
      return false
    case "conditional":
      return mayBeNumber(node.then) || mayBeNumber(node.otherwise)
    default:
      return true
  }
}

// ---- LaTeX (function-plot display) ----
export const LATEX_FUNCTIONS: Record<string, string> = {
  sin: "\\sin",
  cos: "\\cos",
  tan: "\\tan",
  asin: "\\arcsin",
  acos: "\\arccos",
  atan: "\\arctan",
  sinh: "\\sinh",
  cosh: "\\cosh",
  tanh: "\\tanh",
  ln: "\\ln",
  log: "\\ln",
  log10: "\\log_{10}",
  log2: "\\log_{2}",
  exp: "\\exp",
  min: "\\min",
  max: "\\max",
}

export const LATEX_CONSTANTS: Record<string, string> = { pi: "\\pi", e: "e" }

// Variable names that read as Greek letters in formulas (theta, omega_0…).
export const GREEK = new Set([
  "alpha",
  "beta",
  "gamma",
  "delta",
  "epsilon",
  "zeta",
  "eta",
  "theta",
  "iota",
  "kappa",
  "lambda",
  "mu",
  "nu",
  "xi",
  "rho",
  "sigma",
  "tau",
  "upsilon",
  "phi",
  "chi",
  "psi",
  "omega",
  "Gamma",
  "Delta",
  "Theta",
  "Lambda",
  "Xi",
  "Sigma",
  "Phi",
  "Psi",
  "Omega",
])

// `F_g` → F_{g}, `v_0` → v_{0}, `theta` → \theta, `mass` → \mathit{mass}.
// Names are identifiers, so the result never contains model-written LaTeX.
export function variableLatex(name: string): string {
  const [base = name, ...rest] = name.split("_")
  const head = GREEK.has(base)
    ? `\\${base}`
    : base.length > 1
      ? `\\mathit{${base}}`
      : base
  const sub = rest.filter(Boolean).join(",")
  if (!sub) {
    return head
  }
  return `${head}_{${GREEK.has(sub) ? `\\${sub}` : sub.length > 1 ? `\\mathrm{${sub}}` : sub}}`
}

// A number as written in a worked formula: at most four decimals, thousands
// grouped, no exponent for everyday magnitudes.
export function formatLatexNumber(value: number): string {
  if (!Number.isFinite(value)) {
    return "\\text{—}"
  }
  const abs = Math.abs(value)
  if (abs !== 0 && (abs >= 1e15 || abs < 1e-4)) {
    const [mantissa = "0", exponent = "0"] = value.toExponential(3).split("e")
    return `${Number(mantissa)} \\times 10^{${Number(exponent)}}`
  }
  const fixed = Number(value.toFixed(4)).toString()
  const [whole = "0", fraction] = fixed.split(".")
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, "{,}")
  return fraction ? `${grouped}.${fraction}` : grouped
}

export function latexPrecedence(node: Node): number {
  if (node.kind === "binary") {
    return BINARY_PRECEDENCE[node.op] ?? 0
  }
  if (node.kind === "unary") {
    return 5.5
  }
  return 10
}

// Values substituted for variables in a worked formula; absent for the symbolic form.
export type LatexValues = Readonly<Record<string, number>> | undefined

export function wrapLatex(
  node: Node,
  minPrecedence: number,
  values?: LatexValues
): string {
  const latex = nodeToLatex(node, values)
  return latexPrecedence(node) < minPrecedence
    ? `\\left(${latex}\\right)`
    : latex
}
