// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { type Template, POINT_VARIABLES } from "./template"
import { type Expression, type UIValue } from "./expression"
import { BlockSource, BlockIssue, NodeSource } from "./block-1"
import {
  readInitialState,
  orderComputed,
  compileNode,
  implicitState,
} from "./block-2"
import { setOwn } from "./block-4"

export type CompileOptions = {
  // Props shown verbatim, never evaluated (diff before/after), per component.
  rawProps?: (component: string) => readonly string[] | undefined
}

export type CompiledValue =
  | { kind: "static"; value: unknown }
  | { kind: "template"; template: Template }
  | { kind: "point"; template: Template }
  | { kind: "list"; items: CompiledValue[] }
  | { kind: "object"; fields: [string, CompiledValue][] }

export type CompiledNode = {
  component: string
  path: string
  closed: boolean
  bind?: string
  visible?: Expression
  props: [string, CompiledValue][]
  children: CompiledNode[]
}

export type CompiledBlock = {
  source: BlockSource
  // Declared initial state (JSON values), before any user change.
  initialState: Record<string, UIValue>
  // Computed entries in evaluation order; constants are already values.
  computed: { name: string; expression?: Expression; value?: UIValue }[]
  root: CompiledNode
  issues: BlockIssue[]
}

export function compileBlock(
  source: BlockSource,
  options: CompileOptions = {}
): CompiledBlock {
  const issues: BlockIssue[] = []
  const initialState = readInitialState(source.state, issues)
  const computed = orderComputed(
    source.computed,
    new Set(Object.keys(initialState)),
    issues
  )
  // A point variable the block declares itself is an ordinary value, not a per-point one.
  const declared = new Set([
    ...Object.keys(initialState),
    ...computed.map((entry) => entry.name),
  ])
  const pointNames = POINT_VARIABLES.filter((name) => !declared.has(name))
  const context: CompileContext = {
    options,
    issues,
    nodes: 0,
    live: source.live,
    pointNames,
  }
  const root = compileNode(source.root, 0, context) ?? emptyNode(source.root)
  // Controls bound to undeclared state get a default.
  for (const [name, value] of Object.entries(
    implicitState(root, initialState)
  )) {
    setOwn(initialState, name, value)
  }
  return { source, initialState, computed, root, issues }
}

export type CompileContext = {
  options: CompileOptions
  issues: BlockIssue[]
  nodes: number
  live: boolean
  pointNames: readonly string[]
}

export function emptyNode(source: NodeSource): CompiledNode {
  return {
    component: source.component,
    path: source.path,
    closed: source.closed,
    props: [],
    children: [],
  }
}
