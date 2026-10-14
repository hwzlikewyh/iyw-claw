"use client"

import { useMemo, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import {
  parseBlockSource,
  compileBlock,
  resolveBlock,
  hashText,
  type ResolvedNode,
} from "@/lib/intelligent-ui/engine/block"
import { isUIContainer } from "@/lib/intelligent-ui/engine/primitives"
import { BlockFrame } from "./block-frame"
import { UiRenderContext } from "./context"
import { useBlockState } from "./use-block-state"
import { UiNode } from "./node"
import { Widget } from "./widget"
import { useUiScope } from "./scope"

const MAX_SOURCE_BYTES = 200_000

export default function UiBlock({
  source,
  kind,
  closed = false,
  fallback,
}: {
  source: string
  kind: "block" | "widget"
  closed?: boolean
  fallback: ReactNode
}) {
  const scope = useUiScope()
  const live = Boolean(scope?.streaming && !closed)
  const oversized =
    source.length > MAX_SOURCE_BYTES ||
    new TextEncoder().encode(source).length > MAX_SOURCE_BYTES
  const parsed = useMemo(
    () =>
      kind === "block" && !oversized
        ? parseStructuredSource(source, live)
        : null,
    [source, kind, live, oversized]
  )
  if (oversized) return fallback
  if (kind === "widget" && !closed && !scope?.streaming) return fallback
  if (kind === "widget")
    return <Widget source={source} streaming={live} fallback={fallback} />
  if (parsed?.status === "incomplete")
    return (
      <div
        role="status"
        className="my-3 h-24 animate-pulse rounded-lg bg-muted"
      />
    )
  if (!parsed || parsed.status !== "ok") return fallback
  if (parsed.block.version > 2) return fallback
  if (!live && !parsed.block.complete) return fallback
  return (
    <StructuredBlock source={source} block={parsed.block} streaming={live} />
  )
}

function parseStructuredSource(source: string, streaming: boolean) {
  if (!streaming) {
    try {
      JSON.parse(source)
    } catch {
      return { status: "invalid" as const, message: "Expected complete JSON" }
    }
  }
  return parseBlockSource(source, streaming)
}

type StructuredInput = {
  source: string
  block: Parameters<typeof compileBlock>[0]
  streaming: boolean
}
function StructuredBlock(input: StructuredInput) {
  const t = useTranslations("IntelligentUI")
  const { status, context } = useStructuredModel(input)
  const title =
    typeof input.block.root.props.title === "string"
      ? input.block.root.props.title
      : t("title")
  return (
    <BlockFrame
      source={input.source}
      state={status.state}
      status={status}
      reset={() => status.update(null)}
      title={title}
    >
      <UiRenderContext.Provider value={context}>
        <fieldset
          disabled={!status.ready && !input.streaming}
          className="min-w-0 border-0"
        >
          {context.render(context.root)}
        </fieldset>
      </UiRenderContext.Provider>
    </BlockFrame>
  )
}
function useStructuredModel({ source, block, streaming }: StructuredInput) {
  const compiled = useMemo(
    () =>
      compileBlock(block, {
        rawProps: (name) => (name === "diff" ? ["before", "after"] : undefined),
      }),
    [block]
  )
  const status = useBlockState({
    id: block.id || (streaming ? "" : `auto-${hashText(source.trim())}`),
    source,
    defaults: compiled.initialState,
  })
  const resolved = useMemo(
    () => resolveBlock(compiled, status.state),
    [compiled, status.state]
  )
  const render = (node: ResolvedNode): ReactNode =>
    streaming && !node.closed && !isUIContainer(node.component) ? (
      <div className="h-8 animate-pulse rounded bg-muted" />
    ) : (
      <UiNode node={node} />
    )
  const context = {
    root: resolved.root,
    state: status.state,
    values: resolved.values,
    stateNames: new Set(Object.keys(compiled.initialState)),
    update: status.update,
    streaming,
    render,
  }
  return { status, context }
}
