// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import {
  NodeSource,
  CompileContext,
  CompiledNode,
  BLOCK_LIMITS,
  CompiledValue,
  BlockIssue,
} from "./block-1"
import { type Expression, parseExpression, type UIValue } from "./expression"
import { compileTemplate, unwrapExpressionSource } from "./template"
import {
  errorMessage,
  isObject,
  isIdentifier,
  toUIValue,
  setOwn,
} from "./block-4"
import { hasOwn } from "../value"
import { staticValue } from "./block-3"

export function compileNode(
  source: NodeSource,
  depth: number,
  context: CompileContext
): CompiledNode | null {
  if (depth >= BLOCK_LIMITS.depth) {
    context.issues.push({
      path: source.path,
      message: `nesting deeper than ${BLOCK_LIMITS.depth} dropped`,
    })
    return null
  }
  context.nodes += 1
  if (context.nodes > BLOCK_LIMITS.nodes) {
    if (context.nodes === BLOCK_LIMITS.nodes + 1) {
      context.issues.push({
        path: source.path,
        message: `more than ${BLOCK_LIMITS.nodes} nodes; the rest are dropped`,
      })
    }
    return null
  }
  const { props, bind } = compileNodeProps(source, context)
  let visible: Expression | undefined
  if (context.live && source.visible !== undefined) {
    visible = compileCondition(
      source.visible,
      `${source.path}.visible`,
      context.issues
    )
  }
  const children: CompiledNode[] = []
  for (const child of source.children) {
    const compiled = compileNode(child, depth + 1, context)
    if (compiled) {
      children.push(compiled)
    }
  }
  return {
    component: source.component,
    path: source.path,
    closed: source.closed,
    bind,
    visible,
    props,
    children,
  }
}

export function compileValue(
  value: unknown,
  path: string,
  context: Pick<CompileContext, "pointNames" | "issues">
): CompiledValue {
  const { pointNames, issues } = context
  if (typeof value === "string") {
    try {
      const template = compileTemplate(value)
      if (!template) {
        return { kind: "static", value }
      }
      const point = template.variables.some((name) => pointNames.includes(name))
      return point
        ? { kind: "point", template }
        : { kind: "template", template }
    } catch (error) {
      issues.push({ path, message: errorMessage(error) })
      return { kind: "static", value: undefined }
    }
  }
  if (Array.isArray(value)) {
    return {
      kind: "list",
      items: value.map((item, index) =>
        compileValue(item, `${path}[${index}]`, {
          pointNames: pointNames,
          issues: issues,
        })
      ),
    }
  }
  if (isObject(value)) {
    return {
      kind: "object",
      fields: Object.entries(value).map(([key, field]) => [
        key,
        compileValue(field, `${path}.${key}`, {
          pointNames: pointNames,
          issues: issues,
        }),
      ]),
    }
  }
  return { kind: "static", value }
}

export function compileCondition(
  value: unknown,
  path: string,
  issues: BlockIssue[]
): Expression | undefined {
  if (typeof value === "boolean") {
    return parseExpression(String(value))
  }
  if (typeof value !== "string") {
    issues.push({ path, message: "visible must be an expression" })
    return parseExpression("false")
  }
  try {
    return parseExpression(unwrapExpressionSource(value))
  } catch (error) {
    issues.push({ path, message: errorMessage(error) })
    return parseExpression("false")
  }
}

export function readInitialState(
  state: Record<string, unknown> | undefined,
  issues: BlockIssue[]
): Record<string, UIValue> {
  const result: Record<string, UIValue> = {}
  if (!state) {
    return result
  }
  for (const [name, value] of Object.entries(state)) {
    if (!isIdentifier(name)) {
      issues.push({
        path: `state.${name}`,
        message: "state names are letters, digits and _",
      })
      continue
    }
    const json = toUIValue(value, 1)
    if (json === undefined) {
      issues.push({
        path: `state.${name}`,
        message: `state values are JSON, at most ${BLOCK_LIMITS.stateDepth} levels deep`,
      })
      continue
    }
    setOwn(result, name, json)
  }
  return result
}

export const CONTROLS = new Set([
  "slider",
  "select",
  "segmented",
  "switch",
  "input",
  "tabs",
])

export function isControl(component: string): boolean {
  return CONTROLS.has(component)
}

export function implicitState(
  root: CompiledNode,
  declared: Readonly<Record<string, UIValue>>
): Record<string, UIValue> {
  const result: Record<string, UIValue> = {}
  const walk = (node: CompiledNode) => {
    if (
      node.bind &&
      isIdentifier(node.bind) &&
      !hasOwn(declared, node.bind) &&
      !hasOwn(result, node.bind)
    ) {
      const value = controlDefault(node)
      if (value !== undefined) {
        setOwn(result, node.bind, value)
      }
    }
    for (const child of node.children) {
      walk(child)
    }
  }
  walk(root)
  return result
}

export function controlDefault(node: CompiledNode): UIValue | undefined {
  const prop = (name: string): unknown => {
    const entry = node.props.find(([key]) => key === name)?.[1]
    return entry ? staticValue(entry) : undefined
  }
  switch (node.component) {
    case "slider": {
      const min = prop("min")
      return typeof min === "number" ? min : 0
    }
    case "select":
    case "segmented": {
      const options = prop("options")
      const first = Array.isArray(options) ? options[0] : undefined
      if (isObject(first)) {
        const value = toUIValue(first.value, 1)
        return value ?? null
      }
      return toUIValue(first, 1) ?? null
    }
    case "tabs":
      return 0
    case "switch":
      return false
    case "input":
      return prop("type") === "number" ? 0 : ""
    default:
      return undefined
  }
}

export { orderComputed } from "./block-6"

function compileNodeProps(source: NodeSource, context: CompileContext) {
  const raw = new Set(context.options.rawProps?.(source.component) ?? [])
  const props: [string, CompiledValue][] = []
  let bind = source.bind
  for (const [key, value] of Object.entries(source.props)) {
    if (!context.live || raw.has(key)) {
      props.push([key, { kind: "static", value }])
      continue
    }
    // A control written as `"value": "{x}"` binds x.
    if (key === "value" && bind === undefined && typeof value === "string") {
      const bound = /^\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}$/.exec(value)
      if (bound?.[1] && isControl(source.component)) {
        bind = bound[1]
        continue
      }
    }
    props.push([
      key,
      compileValue(value, `${source.path}.${key}`, {
        pointNames: context.pointNames,
        issues: context.issues,
      }),
    ])
  }
  return { props, bind }
}
