"use client"

import { Component, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import type { ResolvedNode } from "@/lib/intelligent-ui/engine/block"
import {
  expandPointProps,
  takeWhere,
  WHERE_LIST_PROP,
} from "@/lib/intelligent-ui/engine/points"
import { PRIMITIVE_COMPONENTS } from "./primitives"
import { hasOwn } from "@/lib/intelligent-ui/value"
import { Control } from "./controls"
import {
  useUiRender,
  useViewState,
  NodePathContext,
  text,
  numeric,
  list,
} from "./context"
import { CatalogComponent } from "./catalog"
import { Scene, Formula } from "./visuals"

const CONTROLS = new Set([
  "slider",
  "input",
  "select",
  "segmented",
  "switch",
  "button",
])
const CONTAINERS = new Set([
  "panel",
  "row",
  "stack",
  "grid",
  "card",
  "tabs",
  "collapse",
])
class NodeBoundary extends Component<
  { children: ReactNode; node: ResolvedNode; fallback: ReactNode },
  { node: ResolvedNode; failed: boolean }
> {
  state = { node: this.props.node, failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  static getDerivedStateFromProps(
    props: { node: ResolvedNode },
    state: { node: ResolvedNode }
  ) {
    return props.node !== state.node
      ? { node: props.node, failed: false }
      : null
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children
  }
}
export function UiNode({ node }: { node: ResolvedNode }) {
  const t = useTranslations("IntelligentUI")
  return (
    <NodePathContext.Provider value={node.path}>
      <NodeBoundary
        node={node}
        fallback={
          <p role="status" className="text-xs text-muted-foreground">
            {t("renderFailed")}
          </p>
        }
      >
        <NodeBody node={node} />
      </NodeBoundary>
    </NodePathContext.Provider>
  )
}
function NodeBody({ node }: { node: ResolvedNode }) {
  if (CONTROLS.has(node.component)) return <Control node={node} />
  if (CONTAINERS.has(node.component)) return <Container node={node} />
  if (hasOwn(PRIMITIVE_COMPONENTS, node.component)) {
    const Primitive =
      PRIMITIVE_COMPONENTS[node.component as keyof typeof PRIMITIVE_COMPONENTS]
    return <Primitive props={node.props} />
  }
  if (node.component === "scene") return <Scene props={node.props} />
  if (node.component === "formula") return <Formula props={node.props} />
  return <CatalogNode node={node} />
}
function CatalogNode({ node }: { node: ResolvedNode }) {
  const expanded = expandPointProps(node.component, node.props, [])
  const filtered = takeWhere(node.component, expanded, [])
  const listProp = WHERE_LIST_PROP[node.component]
  const props =
    filtered.where && listProp
      ? {
          ...filtered.props,
          [listProp]: list(filtered.props[listProp]).filter(filtered.where),
        }
      : filtered.props
  return <CatalogComponent name={node.component} props={props} />
}
function Container({ node }: { node: ResolvedNode }) {
  const { render } = useUiRender()
  const p = node.props
  if (node.component === "tabs") return <Tabs node={node} />
  if (node.component === "collapse") return <Collapse node={node} />
  const className =
    node.component === "grid"
      ? `grid gap-3 ${GRID_COLUMNS[Math.min(4, Math.max(1, Math.trunc(numeric(p.columns, 2))))]}`
      : node.component === "row"
        ? "flex flex-wrap items-start gap-3 [&>*]:min-w-0 [&>*]:flex-1"
        : "space-y-3"
  return (
    <section
      className={
        node.component === "card" ? "rounded-lg border p-3" : "min-w-0"
      }
    >
      {Boolean(p.title) && (
        <h3 className="mb-2 font-semibold">{text(p.title)}</h3>
      )}
      {Boolean(p.description) && (
        <p className="mb-3 text-xs text-muted-foreground">
          {text(p.description)}
        </p>
      )}
      <div className={className}>
        {node.children.map((child) => (
          <div key={child.path}>{render(child)}</div>
        ))}
      </div>
    </section>
  )
}
function Collapse({ node }: { node: ResolvedNode }) {
  const { render } = useUiRender()
  const [open, setOpen] = useViewState<boolean>(
    "open",
    node.props.open === true
  )
  return (
    <details
      open={open}
      onToggle={(event) => {
        if (event.currentTarget.open !== open) setOpen(event.currentTarget.open)
      }}
    >
      <summary className="cursor-pointer font-medium">
        {text(node.props.title)}
      </summary>
      <div className="mt-3 space-y-3">
        {node.children.map((child) => (
          <div key={child.path}>{render(child)}</div>
        ))}
      </div>
    </details>
  )
}
const GRID_COLUMNS: Record<number, string> = {
  1: "grid-cols-1",
  2: "grid-cols-1 sm:grid-cols-2",
  3: "grid-cols-1 sm:grid-cols-3",
  4: "grid-cols-2 sm:grid-cols-4",
}
function Tabs({ node }: { node: ResolvedNode }) {
  const { render, state, update } = useUiRender()
  const [local, setLocal] = useViewState<number>("tab", 0)
  const selected = node.bind ? numeric(state[node.bind]) : local
  const labels = list(node.props.items)
  const child = node.children.find(
    (child) =>
      child.path === (node.path ? `${node.path}.${selected}` : String(selected))
  )
  return (
    <div>
      <div role="tablist" className="mb-3 flex gap-1 overflow-auto">
        {labels.map((label, index) => (
          <button
            type="button"
            role="tab"
            aria-selected={selected === index}
            key={index}
            className={`rounded px-3 py-1.5 text-xs ${selected === index ? "bg-primary text-primary-foreground" : "bg-muted"}`}
            onClick={() =>
              node.bind ? update({ [node.bind]: index }) : setLocal(index)
            }
          >
            {text(label)}
          </button>
        ))}
      </div>
      <div role="tabpanel">{child && render(child)}</div>
    </div>
  )
}
