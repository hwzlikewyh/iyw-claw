"use client"

import { useMemo } from "react"
import { AlertCircle, MoreHorizontal } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"

import {
  useTaskArtifactActions,
  type TaskArtifactActions,
} from "@/components/layout/task-artifact-actions"
import {
  TASK_ARTIFACT_MENU_CONTENT_CLASS,
  TaskArtifactContextMenuItems,
  TaskArtifactDropdownMenuItems,
} from "@/components/layout/task-artifact-menu"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { TaskArtifactTypeIcon } from "@/components/layout/task-artifact-type-icon"
import { useActiveFolder } from "@/contexts/active-folder-context"
import type { TaskArtifactInfo } from "@/lib/api"
import {
  isAbsoluteFilePath,
  toFolderRelativePath,
} from "@/lib/file-path-display"
import { cn } from "@/lib/utils"

interface TaskArtifactFileRowProps {
  item: TaskArtifactInfo
  selected?: boolean
  openOnDoubleClick?: boolean
  onSelect: (item: TaskArtifactInfo) => void
  onOpenWorkspace?: () => void
}

export function TaskArtifactFileRow({
  item,
  selected = false,
  openOnDoubleClick = false,
  onSelect,
  onOpenWorkspace,
}: TaskArtifactFileRowProps) {
  const { activeFolder } = useActiveFolder()
  const locale = useLocale()
  const t = useTranslations("Folder.taskArtifacts")
  const actions = useTaskArtifactActions({
    artifact: item,
    onPreview: onSelect,
    onOpenWorkspace,
  })
  const meta = useMemo(
    () =>
      artifactMetadata({
        path: item.path,
        kind: item.kind,
        createdAt: item.createdAt,
        folderPath: activeFolder?.path,
        locale,
        workspaceRoot: t("workspaceRoot"),
        externalLocation: t("externalLocation"),
      }),
    [activeFolder?.path, item, locale, t]
  )
  const statusLabel = artifactStatusLabel(item.status, t)
  const detail = statusLabel ? `${meta} · ${statusLabel}` : meta

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <ArtifactRowSurface
          item={item}
          meta={detail}
          statusLabel={statusLabel}
          selected={selected}
          openOnDoubleClick={openOnDoubleClick}
          actions={actions}
        />
      </ContextMenuTrigger>
      <ContextMenuContent className={TASK_ARTIFACT_MENU_CONTENT_CLASS}>
        <TaskArtifactContextMenuItems actions={actions} />
      </ContextMenuContent>
    </ContextMenu>
  )
}

function ArtifactRowSurface({
  item,
  meta,
  statusLabel,
  selected,
  openOnDoubleClick,
  actions,
}: {
  item: TaskArtifactInfo
  meta: string
  statusLabel: string | null
  selected: boolean
  openOnDoubleClick: boolean
  actions: TaskArtifactActions
}) {
  return (
    <div
      className={cn(
        "group flex min-w-0 items-center rounded-md pr-1 hover:bg-sidebar-accent",
        selected && "bg-sidebar-accent"
      )}
    >
      <button
        type="button"
        aria-pressed={selected}
        onClick={actions.preview}
        onDoubleClick={
          openOnDoubleClick ? () => void openArtifact(actions) : undefined
        }
        className="flex min-w-0 flex-1 items-center gap-2 px-2 py-2 text-left"
      >
        <ArtifactRowLabel item={item} meta={meta} />
        {statusLabel && <ArtifactStatus label={statusLabel} />}
      </button>
      <ArtifactMoreMenu actions={actions} />
    </div>
  )
}

function ArtifactRowLabel({
  item,
  meta,
}: {
  item: TaskArtifactInfo
  meta: string
}) {
  return (
    <>
      <TaskArtifactTypeIcon item={item} />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{item.displayName}</span>
        <span className="block truncate text-xs text-muted-foreground">
          {meta}
        </span>
      </span>
    </>
  )
}

function ArtifactStatus({ label }: { label: string }) {
  return (
    <span className="shrink-0" role="img" aria-label={label} title={label}>
      <AlertCircle className="size-3.5 text-destructive" aria-hidden="true" />
    </span>
  )
}

function ArtifactMoreMenu({ actions }: { actions: TaskArtifactActions }) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          className="opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
          aria-label={t("moreActions")}
          title={t("moreActions")}
        >
          <MoreHorizontal className="size-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        className={TASK_ARTIFACT_MENU_CONTENT_CLASS}
        align="end"
      >
        <TaskArtifactDropdownMenuItems actions={actions} />
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function openArtifact(actions: TaskArtifactActions) {
  if (actions.canOpenExternal) return actions.openDefault()
  if (actions.canUseSystem) return actions.openDefault()
  if (actions.canOpenWorkspace) return actions.openWorkspace()
  return Promise.resolve()
}

function artifactMetadata({
  path,
  kind,
  createdAt,
  folderPath,
  locale,
  workspaceRoot,
  externalLocation,
}: {
  path: string
  kind: TaskArtifactInfo["kind"]
  createdAt: string
  folderPath?: string
  locale: string
  workspaceRoot: string
  externalLocation: string
}): string {
  if (kind === "url") {
    const time = formatArtifactTime(locale, createdAt)
    try {
      const host = new URL(path).host
      return time ? `${host} · ${time}` : host
    } catch {
      return time ? `${path} · ${time}` : path
    }
  }
  const relative = toFolderRelativePath(path, folderPath)
  const directory = artifactDirectoryLabel(
    relative,
    workspaceRoot,
    externalLocation
  )
  const time = formatArtifactTime(locale, createdAt)
  return time ? `${directory} · ${time}` : directory
}

function artifactDirectoryLabel(
  relativePath: string,
  workspaceRoot: string,
  externalLocation: string
): string {
  if (isAbsoluteFilePath(relativePath)) return externalLocation
  const lastSlash = relativePath.lastIndexOf("/")
  return lastSlash < 0 ? workspaceRoot : relativePath.slice(0, lastSlash)
}

function formatArtifactTime(locale: string, createdAt: string): string {
  const date = new Date(createdAt)
  if (Number.isNaN(date.getTime())) return ""
  return new Intl.DateTimeFormat(locale, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date)
}

function artifactStatusLabel(
  status: TaskArtifactInfo["status"],
  t: ReturnType<typeof useTranslations>
): string | null {
  if (status === "missing") return t("fileMissing")
  if (status === "inaccessible") return t("fileInaccessible")
  return null
}
