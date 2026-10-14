"use client"

import { useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { exportUiSource } from "@/lib/intelligent-ui/export"

export type StateStatus = {
  error: string | null
  retry: (reload?: boolean) => Promise<void>
}
export function StateNotice({ status }: { status: StateStatus }) {
  const t = useTranslations("IntelligentUI")
  if (!status.error) return null
  const conflict = /changed in another view|revision conflict/i.test(
    status.error
  )
  return (
    <div
      role="alert"
      className="flex flex-wrap items-center gap-2 p-3 text-xs text-destructive"
    >
      <span>{t(conflict ? "stateConflict" : "stateSaveFailed")}</span>
      <Button
        size="sm"
        variant="ghost"
        onClick={() => void status.retry().catch(() => {})}
      >
        {t("retry")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        onClick={() => void status.retry(true).catch(() => {})}
      >
        {t("reload")}
      </Button>
    </div>
  )
}
export function SourceView({ source }: { source: string }) {
  return (
    <pre className="max-h-64 overflow-auto whitespace-pre-wrap p-3 text-xs">
      {source}
    </pre>
  )
}
export function BlockFrame({
  source,
  state,
  status,
  reset,
  title,
  children,
}: {
  source: string
  state: Readonly<Record<string, unknown>>
  status: StateStatus
  reset: () => void
  title: string
  children: ReactNode
}) {
  const [expanded, setExpanded] = useState(false),
    [showSource, setShowSource] = useState(false)
  return (
    <div
      className="not-prose my-3 min-w-0 rounded-xl border bg-card text-sm"
      data-intelligent-ui=""
    >
      <div className="min-w-0 p-4">{children}</div>
      <StateNotice status={status} />
      <BlockToolbar
        reset={reset}
        exportSource={() => exportUiSource(source, state)}
        source={() => setShowSource(!showSource)}
        expand={() => setExpanded(true)}
      />
      {showSource && <SourceView source={source} />}
      <Dialog open={expanded} onOpenChange={setExpanded}>
        <DialogContent className="max-h-[90dvh] overflow-auto sm:max-w-4xl">
          <DialogTitle>{title}</DialogTitle>
          {children}
        </DialogContent>
      </Dialog>
    </div>
  )
}
function BlockToolbar(actions: {
  reset: () => void
  exportSource: () => void
  source: () => void
  expand: () => void
}) {
  const t = useTranslations("IntelligentUI")
  return (
    <div className="flex flex-wrap justify-end gap-1 border-t px-2 py-1">
      <Button variant="ghost" size="sm" onClick={actions.exportSource}>
        {t("export")}
      </Button>
      <Button variant="ghost" size="sm" onClick={actions.reset}>
        {t("reset")}
      </Button>
      <Button variant="ghost" size="sm" onClick={actions.source}>
        {t("source")}
      </Button>
      <Button variant="ghost" size="sm" onClick={actions.expand}>
        {t("expand")}
      </Button>
    </div>
  )
}
