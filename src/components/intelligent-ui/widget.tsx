"use client"

import { useRef, useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { widgetKey } from "@/lib/intelligent-ui/engine/fence"
import { Button } from "@/components/ui/button"
import { StateNotice, SourceView } from "./block-frame"
import { Ask } from "./controls"
import { useBlockState } from "./use-block-state"
import { useWidgetBridge } from "./use-widget-bridge"

const DEFAULTS = {}
export function Widget({
  source,
  streaming = false,
  fallback,
}: {
  source: string
  streaming?: boolean
  fallback: ReactNode
}) {
  const t = useTranslations("IntelligentUI")
  if (streaming)
    return (
      <div role="status" className="my-3 h-40 animate-pulse rounded bg-muted">
        {t("generating")}
      </div>
    )
  return <WidgetState key={source} source={source} fallback={fallback} />
}
function WidgetState({
  source,
  fallback,
}: {
  source: string
  fallback: ReactNode
}) {
  const t = useTranslations("IntelligentUI")
  const block = useBlockState({
    id: widgetKey(source),
    source,
    defaults: DEFAULTS,
    freeState: true,
  })
  const [generation, setGeneration] = useState(0)
  if (block.error && !block.ready)
    return (
      <div role="alert" className="text-xs text-destructive">
        {t("stateSaveFailed")}{" "}
        <Button
          size="sm"
          variant="ghost"
          onClick={() => void block.retry(true).catch(() => {})}
        >
          {t("retry")}
        </Button>
        {fallback}
      </div>
    )
  if (!block.ready)
    return (
      <div role="status" className="my-3 h-40 animate-pulse rounded bg-muted">
        {t("generating")}
      </div>
    )
  return (
    <WidgetSurface
      key={generation}
      source={source}
      fallback={fallback}
      block={block}
      restart={() => setGeneration(generation + 1)}
    />
  )
}
type WidgetSurfaceInput = {
  source: string
  fallback: ReactNode
  block: ReturnType<typeof useBlockState>
  restart: () => void
}
function WidgetSurface({
  source,
  fallback,
  block,
  restart,
}: WidgetSurfaceInput) {
  const t = useTranslations("IntelligentUI")
  const frame = useRef<HTMLIFrameElement>(null)
  const bridge = useWidgetBridge(
    { source, state: block.state, update: block.update },
    frame
  )
  const [showSource, setShowSource] = useState(false)
  if (bridge.failed)
    return <WidgetFallback retry={restart}>{fallback}</WidgetFallback>
  const frameProps = {
    srcDoc: bridge.documentHTML,
    title: t("illustration"),
    sandbox: "allow-scripts",
    allow:
      "camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'",
    referrerPolicy: "no-referrer" as const,
    className: "w-full border-0",
    style: { height: bridge.height },
  }
  return (
    <section
      className="not-prose my-3 overflow-hidden rounded-xl border bg-card"
      data-intelligent-ui="widget"
    >
      <iframe ref={frame} {...frameProps} />
      {bridge.suggestion && (
        <div className="p-3">
          <Ask suggestion={bridge.suggestion} />
        </div>
      )}
      <StateNotice status={block} />
      <WidgetToolbar
        reset={() => {
          block.update(null)
          restart()
        }}
        source={() => setShowSource(!showSource)}
      />
      {showSource && <SourceView source={source} />}
    </section>
  )
}
function WidgetFallback({
  retry,
  children,
}: {
  retry: () => void
  children: ReactNode
}) {
  const t = useTranslations("IntelligentUI")
  return (
    <div>
      <p role="alert" className="text-xs text-destructive">
        {t("renderFailed")}{" "}
        <Button variant="ghost" size="sm" onClick={retry}>
          {t("retry")}
        </Button>
      </p>
      {children}
    </div>
  )
}

function WidgetToolbar({
  reset,
  source,
}: {
  reset: () => void
  source: () => void
}) {
  const t = useTranslations("IntelligentUI")
  return (
    <div className="flex justify-end border-t p-1">
      <Button size="sm" variant="ghost" onClick={reset}>
        {t("reset")}
      </Button>
      <Button size="sm" variant="ghost" onClick={source}>
        {t("source")}
      </Button>
    </div>
  )
}
