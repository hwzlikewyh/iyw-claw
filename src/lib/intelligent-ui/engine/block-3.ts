// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import {
  CompiledValue,
  BlockIssue,
  CompiledBlock,
  BLOCK_LIMITS,
  CompiledNode,
} from "./block-1"
import { setOwn, errorMessage, isObject } from "./block-4"
import {
  type UIValue,
  createBudget,
  type ExpressionBudget,
  type ExpressionScope,
  truthy,
} from "./expression"
import { hasOwn } from "../value"
import { type Template, type PointTemplate } from "./template"

export function staticValue(value: CompiledValue): unknown {
  switch (value.kind) {
    case "static":
      return value.value
    case "list": {
      const items: unknown[] = []
      for (const item of value.items) {
        const resolved = staticValue(item)
        if (resolved === undefined) {
          return undefined
        }
        items.push(resolved)
      }
      return items
    }
    case "object": {
      const result: Record<string, unknown> = {}
      for (const [key, field] of value.fields) {
        const resolved = staticValue(field)
        if (resolved === undefined) {
          return undefined
        }
        setOwn(result, key, resolved)
      }
      return result
    }
    default:
      return undefined
  }
}

export type ResolvedNode = {
  component: string
  path: string
  closed: boolean
  bind?: string
  props: Record<string, unknown>
  children: ResolvedNode[]
}

export type ResolvedBlock = {
  root: ResolvedNode
  // State and computed values after this resolution, for display and actions.
  values: Readonly<Record<string, UIValue>>
  issues: BlockIssue[]
}

export function resolveBlock(
  block: CompiledBlock,
  state: Readonly<Record<string, UIValue>>
): ResolvedBlock {
  const issues: BlockIssue[] = []
  const budget = createBudget(BLOCK_LIMITS.steps)
  const values: Record<string, UIValue> = { ...state }
  // A computed entry that fails is left undefined: whatever reads it fails too
  // and shows "—", instead of silently using a stale or made-up value.
  for (const entry of block.computed) {
    if (entry.expression === undefined) {
      if (entry.value !== undefined) {
        setOwn(values, entry.name, entry.value)
      }
      continue
    }
    try {
      setOwn(
        values,
        entry.name,
        entry.expression.evaluate(scopeOf(values, budget))
      )
    } catch (error) {
      issues.push({
        path: `computed.${entry.name}`,
        message: errorMessage(error),
      })
    }
  }
  const context: ResolveContext = { values, budget, issues }
  const root = resolveNode(block.root, context) ?? {
    ...block.root,
    props: {},
    children: [],
  }
  return { root, values, issues }
}

export type ResolveContext = {
  values: Readonly<Record<string, UIValue>>
  budget: ExpressionBudget
  issues: BlockIssue[]
}

export function scopeOf(
  values: Readonly<Record<string, UIValue>>,
  budget: ExpressionBudget,
  point?: Readonly<Record<string, UIValue>>
): ExpressionScope {
  return {
    lookup: (name) => {
      if (point && hasOwn(point, name)) {
        return point[name]
      }
      return hasOwn(values, name) ? values[name] : undefined
    },
    budget,
  }
}

export function resolveNode(
  node: CompiledNode,
  context: ResolveContext
): ResolvedNode | null {
  if (node.visible) {
    try {
      if (
        !truthy(node.visible.evaluate(scopeOf(context.values, context.budget)))
      ) {
        return null
      }
    } catch (error) {
      // A condition that cannot be evaluated hides the node.
      context.issues.push({
        path: `${node.path}.visible`,
        message: errorMessage(error),
      })
      return null
    }
  }
  const props: Record<string, unknown> = {}
  for (const [key, value] of node.props) {
    const resolved = resolveValue(value, `${node.path}.${key}`, context)
    if (resolved !== undefined) {
      setOwn(props, key, resolved)
    }
  }
  const children: ResolvedNode[] = []
  for (const child of node.children) {
    const resolved = resolveNode(child, context)
    if (resolved) {
      children.push(resolved)
    }
  }
  return {
    component: node.component,
    path: node.path,
    closed: node.closed,
    bind: node.bind,
    props,
    children,
  }
}

export function resolveValue(
  value: CompiledValue,
  path: string,
  context: ResolveContext
): unknown {
  switch (value.kind) {
    case "static":
      return value.value
    case "template":
      try {
        // Mixed text degrades per hole; a single-hole value is dropped so the
        // component falls back to its default.
        return value.template.evaluate(
          scopeOf(context.values, context.budget),
          (error) => {
            context.issues.push({ path, message: errorMessage(error) })
          }
        )
      } catch (error) {
        context.issues.push({ path, message: errorMessage(error) })
        return undefined
      }
    case "point":
      return pointTemplate(value.template, context, path)
    case "list": {
      const items: unknown[] = []
      for (const [index, item] of value.items.entries()) {
        const resolved = resolveValue(item, `${path}[${index}]`, context)
        if (resolved !== undefined) {
          items.push(resolved)
        }
      }
      return items
    }
    case "object": {
      const result: Record<string, unknown> = {}
      for (const [key, field] of value.fields) {
        const resolved = resolveValue(field, `${path}.${key}`, context)
        if (resolved !== undefined) {
          setOwn(result, key, resolved)
        }
      }
      return result
    }
  }
}

export function pointTemplate(
  template: Template,
  context: ResolveContext,
  path: string
): PointTemplate {
  let reported = false
  return {
    kind: "point-template",
    source: template.source,
    evaluate: (point) => {
      try {
        return template.evaluate(scopeOf(context.values, context.budget, point))
      } catch (error) {
        if (!reported) {
          reported = true
          context.issues.push({ path, message: errorMessage(error) })
        }
        return null
      }
    },
  }
}

export function isPointTemplate(value: unknown): value is PointTemplate {
  return (
    isObject(value) &&
    value.kind === "point-template" &&
    typeof value.evaluate === "function"
  )
}

export type ResolvedAction =
  | { kind: "set"; patch: Record<string, UIValue> }
  | { kind: "ask"; text: string }
  | { kind: "copy"; text: string }

export { mergeState, stateDiff, sameShape } from "./block-7"
