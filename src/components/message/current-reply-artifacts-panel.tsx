"use client"

import { type ReactNode, useState } from "react"
import { AlertCircle, PackageOpen } from "lucide-react"
import { useTranslations } from "next-intl"

import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import { TaskArtifactTypeIcon } from "@/components/layout/task-artifact-type-icon"
import { artifactVisualKind } from "@/components/layout/task-artifact-type"
import { Button } from "@/components/ui/button"
import type { TaskArtifactInfo } from "@/lib/api"

import { CurrentReplyArtifactsDialog } from "./current-reply-artifacts-dialog"

const VISIBLE_ARTIFACT_COUNT = 4

export function CurrentReplyArtifactsPanel({
  items,
}: {
  items: TaskArtifactInfo[]
}) {
  const [selected, setSelected] = useState<TaskArtifactInfo | null>(null)
  const [allOpen, setAllOpen] = useState(false)
  const visibleItems = items.slice(0, VISIBLE_ARTIFACT_COUNT)

  return (
    <ArtifactPanelSurface
      items={items}
      visibleItems={visibleItems}
      onSelect={setSelected}
      onViewAll={() => setAllOpen(true)}
    >
      <TaskArtifactDialog
        artifact={selected}
        open={selected !== null}
        onOpenChange={(open) => !open && setSelected(null)}
      />
      {allOpen && (
        <CurrentReplyArtifactsDialog
          items={items}
          open
          onOpenChange={setAllOpen}
        />
      )}
    </ArtifactPanelSurface>
  )
}

function ArtifactPanelSurface({
  items,
  visibleItems,
  onSelect,
  onViewAll,
  children,
}: {
  items: TaskArtifactInfo[]
  visibleItems: TaskArtifactInfo[]
  onSelect: (item: TaskArtifactInfo) => void
  onViewAll: () => void
  children: ReactNode
}) {
  const t = useTranslations("Folder.taskArtifacts")
  const remaining = items.length - visibleItems.length

  return (
    <section
      aria-label={t("currentReplyTitle")}
      className="mt-3 min-w-0 max-w-full overflow-hidden border-t border-border/60 pt-3"
    >
      <ArtifactPanelHeader
        count={items.length}
        remaining={remaining}
        onViewAll={onViewAll}
      />
      <div className="max-w-full overflow-x-auto overscroll-x-contain pb-1">
        <div className="grid min-w-[49.5rem] grid-cols-4 gap-2">
          {visibleItems.map((item) => (
            <ArtifactTile key={item.id} item={item} onSelect={onSelect} />
          ))}
        </div>
      </div>
      {children}
    </section>
  )
}

function ArtifactPanelHeader({
  count,
  remaining,
  onViewAll,
}: {
  count: number
  remaining: number
  onViewAll: () => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="mb-1.5 flex min-w-0 items-center gap-2 overflow-hidden px-1 whitespace-nowrap">
      <PackageOpen className="size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate text-xs font-medium">
        {t("currentReplyTitle")}
      </span>
      <span className="shrink-0 text-[11px] text-muted-foreground">
        {t("currentReplyCount", { count })}
      </span>
      {remaining > 0 && (
        <span className="flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
          <span>{t("currentReplyRemaining", { count: remaining })}</span>
          <span aria-hidden="true">·</span>
          <Button
            type="button"
            variant="link"
            className="h-auto p-0 text-[11px]"
            onClick={onViewAll}
          >
            {t("currentReplyViewAll")}
          </Button>
        </span>
      )}
    </div>
  )
}

function ArtifactTile({
  item,
  onSelect,
}: {
  item: TaskArtifactInfo
  onSelect: (item: TaskArtifactInfo) => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  const status = artifactStatus(item, t)
  return (
    <button
      type="button"
      className="flex min-h-16 min-w-0 items-center gap-2 rounded-md border bg-card/40 px-3 py-2 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      onClick={() => onSelect(item)}
    >
      <TaskArtifactTypeIcon item={item} size="md" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium">
          {item.displayName}
        </span>
        <span className="block truncate text-xs text-muted-foreground">
          {artifactType(item, t)}
        </span>
        <span className="mt-0.5 flex items-center gap-1 truncate text-[11px] text-muted-foreground">
          {item.status !== "available" && (
            <AlertCircle
              aria-hidden="true"
              className="size-3 shrink-0 text-destructive"
            />
          )}
          <span className="truncate">{status}</span>
        </span>
      </span>
    </button>
  )
}

function artifactType(
  item: TaskArtifactInfo,
  t: ReturnType<typeof useTranslations>
) {
  switch (artifactVisualKind(item)) {
    case "folder":
      return t("currentReplyTypeFolder")
    case "link":
      return t("currentReplyTypeLink")
    case "image":
      return t("currentReplyTypeImage")
    case "video":
      return t("currentReplyTypeVideo")
    case "audio":
      return t("currentReplyTypeAudio")
    case "data":
      return t("currentReplyTypeData")
    case "font":
      return t("currentReplyTypeFont")
    case "database":
      return t("currentReplyTypeDatabase")
    case "spreadsheet":
      return t("currentReplyTypeSpreadsheet")
    case "archive":
      return t("currentReplyTypeArchive")
    case "document":
      return t("currentReplyTypeDocument")
    case "code":
      return t("currentReplyTypeCode")
    default:
      return t("currentReplyTypeFile")
  }
}

function artifactStatus(
  item: TaskArtifactInfo,
  t: ReturnType<typeof useTranslations>
) {
  if (item.status === "missing") return t("fileMissing")
  if (item.status === "inaccessible") return t("fileInaccessible")
  return t("currentReplyStatusAvailable")
}
