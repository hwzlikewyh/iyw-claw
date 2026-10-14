import { hasOwn } from "../value"
// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import {
  Token,
  Node,
  ExpressionError,
  BINARY_PRECEDENCE,
  POWER_PRECEDENCE,
  KEYWORDS,
  UIValue,
  DEFAULT_STEPS,
  CONSTANTS,
} from "./expression-1"
import {
  FUNCTIONS,
  truthy,
  toNumber,
  evaluateBinary,
  readMember,
} from "./expression-3"

export class Parser {
  private index = 0
  private readonly tokens: Token[]

  constructor(tokens: Token[]) {
    this.tokens = tokens
  }

  parse(): Node {
    const node = this.conditional()
    if (this.peek().kind !== "end") {
      throw new ExpressionError("unexpected trailing input")
    }
    return node
  }

  private peek(): Token {
    return this.tokens[this.index] ?? { kind: "end" }
  }

  private next(): Token {
    const token = this.peek()
    this.index += 1
    return token
  }

  private isPunct(value: string): boolean {
    const token = this.peek()
    return token.kind === "punct" && token.value === value
  }

  private expect(value: string): void {
    const token = this.next()
    if (token.kind !== "punct" || token.value !== value) {
      throw new ExpressionError(`expected ${value}`)
    }
  }

  private conditional(): Node {
    const test = this.binary(0)
    if (!this.isPunct("?")) {
      return test
    }
    this.next()
    const then = this.conditional()
    this.expect(":")
    const otherwise = this.conditional()
    return { kind: "conditional", test, then, otherwise }
  }

  private binary(minPrecedence: number): Node {
    let left = this.unary()
    for (;;) {
      const token = this.peek()
      if (token.kind !== "op") {
        return left
      }
      const precedence = BINARY_PRECEDENCE[token.value]
      if (precedence === undefined || precedence < minPrecedence) {
        return left
      }
      this.next()
      const right = this.binary(
        token.value === "^" ? precedence : precedence + 1
      )
      left =
        token.value === "&&" || token.value === "||"
          ? { kind: "logical", op: token.value, left, right }
          : { kind: "binary", op: token.value, left, right }
    }
  }

  private unary(): Node {
    const token = this.peek()
    if (token.kind === "op" && token.value === "-") {
      this.next()
      // Unary minus binds looser than ^, so -2^2 is -(2^2) as in most math tools.
      return { kind: "unary", op: "-", operand: this.binary(POWER_PRECEDENCE) }
    }
    if (token.kind === "op" && token.value === "!") {
      this.next()
      return { kind: "unary", op: "!", operand: this.unary() }
    }
    if (token.kind === "op" && token.value === "+") {
      this.next()
      return this.unary()
    }
    return this.postfix(this.primary())
  }

  private postfix(node: Node): Node {
    let current = node
    for (;;) {
      if (this.isPunct(".")) {
        this.next()
        const name = this.next()
        if (name.kind !== "ident") {
          throw new ExpressionError("expected property name")
        }
        current = {
          kind: "member",
          object: current,
          property: { kind: "literal", value: name.value },
          computed: false,
        }
      } else if (this.isPunct("[")) {
        this.next()
        const property = this.conditional()
        this.expect("]")
        current = { kind: "member", object: current, property, computed: true }
      } else {
        return current
      }
    }
  }

  private primary(): Node {
    const token = this.next()
    switch (token.kind) {
      case "number":
      case "string":
        return { kind: "literal", value: token.value }
      case "ident": {
        if (this.isPunct("(")) {
          this.next()
          const args = this.list(")")
          if (!hasOwn(FUNCTIONS, token.value)) {
            throw new ExpressionError(`unknown function ${token.value}`)
          }
          return { kind: "call", name: token.value, args }
        }
        if (hasOwn(KEYWORDS, token.value)) {
          return { kind: "literal", value: KEYWORDS[token.value] ?? null }
        }
        return { kind: "var", name: token.value }
      }
      case "punct":
        if (token.value === "(") {
          const node = this.conditional()
          this.expect(")")
          return node
        }
        if (token.value === "[") {
          return { kind: "array", items: this.list("]") }
        }
        throw new ExpressionError(`unexpected "${token.value}"`)
      default:
        throw new ExpressionError("unexpected end of expression")
    }
  }

  private list(close: ")" | "]"): Node[] {
    const items: Node[] = []
    if (this.isPunct(close)) {
      this.next()
      return items
    }
    items.push(this.conditional())
    while (this.isPunct(",")) {
      this.next()
      items.push(this.conditional())
    }
    this.expect(close)
    return items
  }
}

// ---- Evaluation ----

export type ExpressionBudget = { steps: number }

export type ExpressionScope = {
  // Returns undefined for a name the scope does not define.
  lookup: (name: string) => UIValue | undefined
  budget: ExpressionBudget
}

export function createBudget(steps = DEFAULT_STEPS): ExpressionBudget {
  return { steps }
}

export function spend(budget: ExpressionBudget, amount = 1): void {
  budget.steps -= amount
  if (budget.steps < 0) {
    throw new ExpressionError("expression budget exceeded")
  }
}

export function evaluateNode(node: Node, scope: ExpressionScope): UIValue {
  spend(scope.budget)
  switch (node.kind) {
    case "literal":
      return node.value
    case "array":
      return node.items.map((item) => evaluateNode(item, scope))
    case "var":
      return resolveVariable(node.name, scope)
    case "unary": {
      const operand = evaluateNode(node.operand, scope)
      return node.op === "!" ? !truthy(operand) : -toNumber(operand, "-")
    }
    case "logical": {
      const left = evaluateNode(node.left, scope)
      if (node.op === "&&") {
        return truthy(left) ? evaluateNode(node.right, scope) : left
      }
      return truthy(left) ? left : evaluateNode(node.right, scope)
    }
    case "conditional":
      return truthy(evaluateNode(node.test, scope))
        ? evaluateNode(node.then, scope)
        : evaluateNode(node.otherwise, scope)
    case "binary":
      return evaluateBinary(
        node.op,
        evaluateNode(node.left, scope),
        evaluateNode(node.right, scope)
      )
    case "member":
      return readMember(
        evaluateNode(node.object, scope),
        evaluateNode(node.property, scope)
      )
    case "call": {
      const fn = hasOwn(FUNCTIONS, node.name) ? FUNCTIONS[node.name] : undefined
      if (!fn) {
        throw new ExpressionError(`unknown function ${node.name}`)
      }
      return fn(
        node.args.map((arg) => evaluateNode(arg, scope)),
        scope.budget
      )
    }
  }
}

function resolveVariable(name: string, scope: ExpressionScope): UIValue {
  const value = scope.lookup(name)
  if (value !== undefined) return value
  if (hasOwn(CONSTANTS, name)) return CONSTANTS[name]
  throw new ExpressionError(`unknown variable ${name}`)
}
