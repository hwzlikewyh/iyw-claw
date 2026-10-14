// 基于 DEEIX-Chat Apache-2.0 引擎，出处见 ../UPSTREAM.md。
import { parsePartialJSON } from "./partial-json"
import { isObject, setOwn } from "./block-4"

export const BLOCK_LIMITS = {
  computed: 64,
  nodes: 300,
  depth: 8,
  // Steps shared by every formula of one resolution, including per-point ones.
  steps: 200_000,
  stateDepth: 4,
  askLength: 120,
} as const

export const STRUCTURE_KEYS = new Set([
  "component",
  "id",
  "version",
  "state",
  "computed",
  "children",
  "visible",
  "bind",
  "props",
])

export type BlockIssue = { path: string; message: string }

export type NodeSource = {
  component: string
  props: Record<string, unknown>
  children: NodeSource[]
  bind?: string
  visible?: unknown
  // The node's closing brace has been read; leaves render only once closed.
  closed: boolean
  // Index path from the root, stable while the block streams in.
  path: string
}

export type BlockSource = {
  component: string
  // Model-given id when valid (`[A-Za-z0-9_-]`, at most 64), otherwise "".
  id: string
  // What state is stored and injected under: the id, or for a block without a
  // valid one, `auto-<hash>` of its content once complete ("" while streaming).
  key: string
  version: number
  state: Record<string, unknown> | undefined
  computed: Record<string, unknown> | undefined
  root: NodeSource
  // The whole block has been read.
  complete: boolean
  // A live block: has state, computed or children.
  live: boolean
}

export type ParsedBlockSource =
  | { status: "incomplete" }
  | { status: "invalid"; message: string }
  | { status: "ok"; block: BlockSource }

export function parseBlockSource(
  raw: string,
  streaming: boolean
): ParsedBlockSource {
  let parsed: ReturnType<typeof parsePartialJSON>
  try {
    parsed = parsePartialJSON(raw)
  } catch (error) {
    return {
      status: "invalid",
      message: error instanceof Error ? error.message : "invalid JSON",
    }
  }
  const { value, closed } = parsed
  if (
    !isObject(value) ||
    typeof value.component !== "string" ||
    value.component === ""
  ) {
    if (streaming && !parsed.complete) {
      return { status: "incomplete" }
    }
    return { status: "invalid", message: "expected an object with a component" }
  }
  if (!parsed.complete && parsed.openTail.get(value) === "component") {
    return { status: "incomplete" }
  }

  const state = isObject(value.state) ? value.state : undefined
  const computed = isObject(value.computed) ? value.computed : undefined
  const root = readNode(value, "", closed)
  const id = normalizeID(value.id)
  return {
    status: "ok",
    block: {
      component: value.component,
      id,
      key: id || (parsed.complete ? contentKey(value) : ""),
      version: normalizeVersion(value.version),
      state,
      computed,
      root,
      complete: parsed.complete,
      live:
        state !== undefined ||
        computed !== undefined ||
        Array.isArray(value.children),
    },
  }
}

export function readNode(
  value: Record<string, unknown>,
  path: string,
  closed: WeakSet<object>
): NodeSource {
  // Props sit either in a `props` object or flat on the node.
  const props: Record<string, unknown> = {}
  const legacy = isObject(value.props) ? value.props : undefined
  for (const [key, field] of Object.entries(legacy ?? value)) {
    if (legacy || !STRUCTURE_KEYS.has(key)) {
      setOwn(props, key, field)
    }
  }
  const children: NodeSource[] = []
  if (
    Array.isArray(value.children) &&
    path.split(".").length < BLOCK_LIMITS.depth
  ) {
    value.children.slice(0, BLOCK_LIMITS.nodes).forEach((child, index) => {
      if (
        isObject(child) &&
        typeof child.component === "string" &&
        child.component !== ""
      ) {
        children.push(
          readNode(child, path ? `${path}.${index}` : String(index), closed)
        )
      }
    })
  }
  return {
    component: typeof value.component === "string" ? value.component : "",
    props,
    children,
    bind:
      typeof value.bind === "string" && value.bind !== ""
        ? value.bind
        : undefined,
    visible: value.visible,
    closed: closed.has(value),
    path,
  }
}

export function normalizeVersion(version: unknown): number {
  if (typeof version === "number" && Number.isFinite(version)) {
    return Math.max(1, Math.trunc(version))
  }
  if (typeof version === "string") {
    const match = /^v?(\d+)(?:\.\d+)?$/i.exec(version.trim())
    if (match) {
      return Number.parseInt(match[1] ?? "1", 10)
    }
  }
  return 1
}

export const BLOCK_ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/

export function normalizeID(id: unknown): string {
  const text =
    typeof id === "number" && Number.isFinite(id)
      ? String(id)
      : typeof id === "string"
        ? id.trim()
        : ""
  return BLOCK_ID_PATTERN.test(text) ? text : ""
}

export function contentKey(value: unknown): string {
  return `auto-${hashText(JSON.stringify(value))}`
}

export function hashText(text: string): string {
  let hash = 0x811c9dc5
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index)
    hash = Math.imul(hash, 0x01000193)
  }
  return (hash >>> 0).toString(36)
}

export {
  type CompileOptions,
  type CompiledValue,
  type CompiledNode,
  type CompiledBlock,
  compileBlock,
  type CompileContext,
  emptyNode,
} from "./block-5"
