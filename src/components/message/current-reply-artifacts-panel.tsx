"use client"

import { type ReactNode, useState } from "react"
import {
  AlertCircle,
  File,
  FileArchive,
  FileCode2,
  FileImage,
  FileSpreadsheet,
  FileText,
  Folder,
  Link,
  PackageOpen,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import { Button } from "@/components/ui/button"
import { isImageFile, languageFromPath } from "@/lib/language-detect"
import type { TaskArtifactInfo } from "@/lib/api"

import { CurrentReplyArtifactsDialog } from "./current-reply-artifacts-dialog"

const VISIBLE_ARTIFACT_COUNT = 4
const ARCHIVE_EXTENSIONS = new Set(["zip", "rar", "7z", "tar", "gz", "bz2"])
const SPREADSHEET_EXTENSIONS = new Set(["xls", "xlsx", "csv", "tsv"])
const DOCUMENT_EXTENSIONS = new Set([
  "txt",
  "md",
  "pdf",
  "doc",
  "docx",
  "ppt",
  "pptx",
])

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
      <ArtifactTypeIcon item={item} />
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

function ArtifactTypeIcon({ item }: { item: TaskArtifactInfo }) {
  const className = "size-5 shrink-0 text-muted-foreground"
  if (item.kind === "directory") return <Folder className={className} />
  if (item.kind === "url") return <Link className={className} />
  if (isImageFile(item.path)) return <FileImage className={className} />
  const extension = artifactExtension(item.path)
  if (SPREADSHEET_EXTENSIONS.has(extension)) {
    return <FileSpreadsheet className={className} />
  }
  if (ARCHIVE_EXTENSIONS.has(extension)) {
    return <FileArchive className={className} />
  }
  if (DOCUMENT_EXTENSIONS.has(extension))
    return <FileText className={className} />
  if (languageFromPath(item.path) !== "plaintext") {
    return <FileCode2 className={className} />
  }
  return <File className={className} />
}

function artifactType(
  item: TaskArtifactInfo,
  t: ReturnType<typeof useTranslations>
) {
  if (item.kind === "directory") return t("currentReplyTypeFolder")
  if (item.kind === "url") return t("currentReplyTypeLink")
  if (isImageFile(item.path)) return t("currentReplyTypeImage")
  const extension = artifactExtension(item.path)
  if (SPREADSHEET_EXTENSIONS.has(extension)) {
    return t("currentReplyTypeSpreadsheet")
  }
  if (ARCHIVE_EXTENSIONS.has(extension)) return t("currentReplyTypeArchive")
  if (DOCUMENT_EXTENSIONS.has(extension)) return t("currentReplyTypeDocument")
  if (languageFromPath(item.path) !== "plaintext") {
    return t("currentReplyTypeCode")
  }
  return t("currentReplyTypeFile")
}

function artifactStatus(
  item: TaskArtifactInfo,
  t: ReturnType<typeof useTranslations>
) {
  if (item.status === "missing") return t("fileMissing")
  if (item.status === "inaccessible") return t("fileInaccessible")
  return t("currentReplyStatusAvailable")
}

function artifactExtension(path: string): string {
  return path.split(".").pop()?.toLowerCase() ?? ""
}
