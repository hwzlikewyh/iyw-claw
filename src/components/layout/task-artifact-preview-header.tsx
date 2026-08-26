"use client"

import type { ReactNode } from "react"
import {
  ArrowLeft,
  ExternalLink,
  Fullscreen,
  FolderSearch,
  Maximize2,
  Minimize2,
  MoreHorizontal,
  Waypoints,
} from "lucide-react"
import { useTranslations } from "next-intl"

import type { TaskArtifactActions } from "@/components/layout/task-artifact-actions"
import { TaskArtifactTypeIcon } from "@/components/layout/task-artifact-type-icon"
import {
  TASK_ARTIFACT_MENU_CONTENT_CLASS,
  TaskArtifactDropdownMenuItems,
} from "@/components/layout/task-artifact-menu"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { TaskArtifactInfo } from "@/lib/api"

interface TaskArtifactPreviewHeaderProps {
  artifact: TaskArtifactInfo
  actions: TaskArtifactActions
  onBack?: () => void
  isAppFullscreen?: boolean
  isSystemFullscreen?: boolean
  onToggleAppFullscreen?: () => void
  onToggleSystemFullscreen?: () => Promise<void>
}

interface ArtifactHeaderActionsProps {
  actions: TaskArtifactActions
  isAppFullscreen: boolean
  isSystemFullscreen: boolean
  onToggleAppFullscreen?: () => void
  onToggleSystemFullscreen?: () => Promise<void>
}

export function TaskArtifactPreviewHeader({
  artifact,
  actions,
  onBack,
  isAppFullscreen = false,
  isSystemFullscreen = false,
  onToggleAppFullscreen,
  onToggleSystemFullscreen,
}: TaskArtifactPreviewHeaderProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const subtitle =
    artifact.kind === "directory"
      ? t("folderArtifact")
      : artifact.kind === "url"
        ? artifact.path
        : (actions.target?.ioPath ?? artifact.displayName)
  return (
    <header className="flex h-12 min-w-0 items-center gap-1 border-b px-3 pr-12">
      {onBack && (
        <Button
          variant="ghost"
          size="icon-sm"
          className="md:hidden"
          onClick={onBack}
          aria-label={t("backToList")}
          title={t("backToList")}
        >
          <ArrowLeft className="size-4" />
        </Button>
      )}
      <TaskArtifactTypeIcon item={artifact} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{artifact.displayName}</p>
        <p className="truncate text-xs text-muted-foreground">{subtitle}</p>
      </div>
      <ArtifactHeaderActions
        actions={actions}
        isAppFullscreen={isAppFullscreen}
        isSystemFullscreen={isSystemFullscreen}
        onToggleAppFullscreen={onToggleAppFullscreen}
        onToggleSystemFullscreen={onToggleSystemFullscreen}
      />
    </header>
  )
}

function ArtifactHeaderActions({
  actions,
  isAppFullscreen,
  isSystemFullscreen,
  onToggleAppFullscreen,
  onToggleSystemFullscreen,
}: ArtifactHeaderActionsProps) {
  return (
    <>
      <ArtifactFullscreenActions
        isAppFullscreen={isAppFullscreen}
        isSystemFullscreen={isSystemFullscreen}
        onToggleAppFullscreen={onToggleAppFullscreen}
        onToggleSystemFullscreen={onToggleSystemFullscreen}
      />
      <ArtifactLocationActions actions={actions} />
      <ArtifactMoreMenu actions={actions} />
    </>
  )
}

function ArtifactFullscreenActions({
  isAppFullscreen,
  isSystemFullscreen,
  onToggleAppFullscreen,
  onToggleSystemFullscreen,
}: Omit<ArtifactHeaderActionsProps, "actions">) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <>
      {onToggleAppFullscreen && (
        <ArtifactActionButton
          label={
            isAppFullscreen
              ? t("exitPreviewFullscreen")
              : t("previewFullscreen")
          }
          icon={
            isAppFullscreen ? (
              <Minimize2 className="size-4" />
            ) : (
              <Maximize2 className="size-4" />
            )
          }
          onClick={onToggleAppFullscreen}
        />
      )}
      {isAppFullscreen && onToggleSystemFullscreen && (
        <ArtifactActionButton
          label={
            isSystemFullscreen
              ? t("exitSystemFullscreen")
              : t("systemFullscreen")
          }
          icon={
            isSystemFullscreen ? (
              <Minimize2 className="size-4" />
            ) : (
              <Fullscreen className="size-4" />
            )
          }
          onClick={onToggleSystemFullscreen}
        />
      )}
    </>
  )
}

function ArtifactLocationActions({
  actions,
}: {
  actions: TaskArtifactActions
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <>
      {(actions.canUseSystem || actions.canOpenExternal) && (
        <ArtifactActionButton
          label={actions.canOpenExternal ? t("openExternal") : t("openDefault")}
          icon={<ExternalLink className="size-4" />}
          onClick={actions.openDefault}
        />
      )}
      {actions.canUseSystem && (
        <ArtifactActionButton
          label={t("reveal")}
          icon={<FolderSearch className="size-4" />}
          onClick={actions.reveal}
        />
      )}
      {!actions.canUseSystem && actions.canOpenWorkspace && (
        <ArtifactActionButton
          label={t("openWorkspace")}
          icon={<Waypoints className="size-4" />}
          onClick={actions.openWorkspace}
        />
      )}
    </>
  )
}

function ArtifactMoreMenu({ actions }: { actions: TaskArtifactActions }) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("moreActions")}
          title={t("moreActions")}
        >
          <MoreHorizontal className="size-4" />
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

function ArtifactActionButton({
  label,
  icon,
  onClick,
}: {
  label: string
  icon: ReactNode
  onClick: () => void | Promise<void>
}) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={() => void onClick()}
      aria-label={label}
      title={label}
    >
      {icon}
    </Button>
  )
}
