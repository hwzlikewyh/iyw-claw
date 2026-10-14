// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { BlockIssue, CompiledBlock, BLOCK_LIMITS } from "./block-1"
import { isIdentifier, errorMessage, toUIValue } from "./block-4"
import { type Expression, type UIValue, parseExpression } from "./expression"
import { unwrapExpressionSource } from "./template"

export function orderComputed(
  computed: Record<string, unknown> | undefined,
  stateNames: ReadonlySet<string>,
  issues: BlockIssue[]
): CompiledBlock["computed"] {
  if (!computed) {
    return []
  }
  const compiled = compileComputed(
    readComputed(computed, stateNames, issues),
    issues
  )
  const order: CompiledBlock["computed"] = []
  const visiting = new Set<string>()
  const done = new Set<string>()
  const visit = (name: string, trail: string[]): void => {
    if (done.has(name)) {
      return
    }
    if (visiting.has(name)) {
      issues.push({
        path: `computed.${name}`,
        message: `circular: ${[...trail, name].join(" → ")}`,
      })
      return
    }
    const entry = compiled.get(name)
    if (!entry) {
      return
    }
    visiting.add(name)
    for (const dependency of entry.expression?.variables ?? []) {
      if (compiled.has(dependency)) {
        visit(dependency, [...trail, name])
      }
    }
    visiting.delete(name)
    done.add(name)
    order.push({ name, ...entry })
  }
  for (const name of compiled.keys()) {
    visit(name, [])
  }
  return order
}

function readComputed(
  computed: Record<string, unknown>,
  stateNames: ReadonlySet<string>,
  issues: BlockIssue[]
) {
  const entries = Object.entries(computed).filter(([name]) => {
    if (!isIdentifier(name)) {
      issues.push({
        path: `computed.${name}`,
        message: "computed names are letters, digits and _",
      })
      return false
    }
    if (stateNames.has(name)) {
      issues.push({
        path: `computed.${name}`,
        message: "already declared in state",
      })
      return false
    }
    return true
  })
  if (entries.length > BLOCK_LIMITS.computed) {
    issues.push({
      path: "computed",
      message: `more than ${BLOCK_LIMITS.computed} entries; the rest are dropped`,
    })
    entries.length = BLOCK_LIMITS.computed
  }

  return entries
}
function compileComputed(entries: [string, unknown][], issues: BlockIssue[]) {
  const compiled = new Map<
    string,
    { expression?: Expression; value?: UIValue }
  >()
  for (const [name, value] of entries) {
    if (typeof value === "string") {
      try {
        compiled.set(name, {
          expression: parseExpression(unwrapExpressionSource(value)),
        })
      } catch (error) {
        issues.push({ path: `computed.${name}`, message: errorMessage(error) })
        compiled.set(name, { expression: undefined })
      }
      continue
    }
    // Non-string values are constants: lookup tables, lists, flags.
    const json = toUIValue(value, 1)
    if (json === undefined) {
      issues.push({
        path: `computed.${name}`,
        message: "constant is not a JSON value",
      })
    }
    compiled.set(name, { value: json ?? null })
  }

  return compiled
}
