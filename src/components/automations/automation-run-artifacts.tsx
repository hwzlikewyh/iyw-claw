"use client"

import { useEffect, useRef, useState } from "react"
import { AlertCircle, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { TaskArtifactTypeIcon } from "@/components/layout/task-artifact-type-icon"
import { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { ResourcePreview } from "@/components/resources/resource-preview"
import { Button } from "@/components/ui/button"
import type { TaskArtifactInfo } from "@/lib/api"

export function AutomationRunArtifacts({
  conversationId,
  status,
}: {
  conversationId: number | null
  status: string
}) {
  if (conversationId == null) return null
  return (
    <RunArtifacts
      key={conversationId}
      conversationId={conversationId}
      status={status}
    />
  )
}

function RunArtifacts(props: { conversationId: number; status: string }) {
  const [selected, setSelected] = useState<TaskArtifactInfo | null>(null)
  const previousStatus = useRef(props.status)
  const query = useTaskArtifacts({
    conversationId: props.conversationId,
    folderId: null,
    scope: "current",
    loadAll: true,
  })
  const { refresh } = query
  useEffect(() => {
    if (previousStatus.current === props.status) return
    previousStatus.current = props.status
    void refresh()
  }, [refresh, props.status])

  if (!query.loading && !query.error && !query.items.length) return null
  return (
    <div className="mt-2 min-w-0 max-w-lg whitespace-normal text-xs">
      <ArtifactList query={query} onSelect={setSelected} />
      <ResourcePreview artifact={selected} onClose={() => setSelected(null)} />
    </div>
  )
}

function ArtifactList({
  query,
  onSelect,
}: {
  query: ReturnType<typeof useTaskArtifacts>
  onSelect: (item: TaskArtifactInfo) => void
}) {
  const t = useTranslations("Folder.taskArtifacts")

  if (query.loading && !query.items.length) {
    return (
      <span
        role="status"
        className="inline-flex items-center gap-1 text-muted-foreground"
      >
        <Loader2 className="size-3 animate-spin" aria-hidden="true" />
        {t("loading")}
      </span>
    )
  }
  if (query.error && !query.items.length) {
    return <ArtifactLoadError retry={query.refresh} />
  }
  if (!query.items.length) return null

  return (
    <div className="flex max-h-44 flex-col gap-1 overflow-y-auto">
      <span className="shrink-0 text-muted-foreground">
        {t("open")} ({query.total})
      </span>
      {query.items.map((item) => (
        <ArtifactLink key={item.id} item={item} onClick={onSelect} />
      ))}
    </div>
  )
}

function ArtifactLoadError({ retry }: { retry: () => Promise<void> }) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <span
      role="alert"
      className="inline-flex items-center gap-2 text-destructive"
    >
      <AlertCircle className="size-3" aria-hidden="true" />
      {t("loadFailed")}
      <Button
        type="button"
        size="sm"
        variant="link"
        className="h-auto p-0 text-xs"
        onClick={() => void retry()}
      >
        {t("retry")}
      </Button>
    </span>
  )
}

function ArtifactLink({
  item,
  onClick,
}: {
  item: TaskArtifactInfo
  onClick: (item: TaskArtifactInfo) => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  const unavailable =
    item.status === "missing"
      ? t("fileMissing")
      : item.status === "inaccessible"
        ? t("fileInaccessible")
        : null
  return (
    <button
      type="button"
      className="inline-flex min-w-0 shrink-0 items-center gap-1.5 text-left text-primary hover:underline"
      title={
        unavailable ? `${item.displayName} · ${unavailable}` : item.displayName
      }
      onClick={() => onClick(item)}
    >
      <TaskArtifactTypeIcon item={item} className="size-4 rounded-sm" />
      <span className="truncate">{item.displayName}</span>
      {unavailable && (
        <AlertCircle
          className="size-3 shrink-0 text-destructive"
          aria-label={unavailable}
        />
      )}
    </button>
  )
}
