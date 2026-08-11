"use client"

import type { ReactNode } from "react"
import {
  ArrowLeft,
  ExternalLink,
  FolderSearch,
  MoreHorizontal,
  PanelsTopLeft,
  Waypoints,
} from "lucide-react"
import { useTranslations } from "next-intl"

import type { TaskArtifactActions } from "@/components/layout/task-artifact-actions"
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

export function TaskArtifactPreviewHeader({
  artifact,
  actions,
  onBack,
}: {
  artifact: TaskArtifactInfo
  actions: TaskArtifactActions
  onBack?: () => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  const subtitle = actions.target?.ioPath ?? t("previewOutsideWorkspace")
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
      <PanelsTopLeft className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{artifact.displayName}</p>
        <p
          className="truncate text-xs text-muted-foreground"
          title={artifact.path}
        >
          {subtitle}
        </p>
      </div>
      <ArtifactHeaderActions actions={actions} />
    </header>
  )
}

function ArtifactHeaderActions({ actions }: { actions: TaskArtifactActions }) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <>
      {actions.canUseSystem && (
        <ArtifactActionButton
          label={t("openDefault")}
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
    </>
  )
}

function ArtifactActionButton({
  label,
  icon,
  onClick,
}: {
  label: string
  icon: ReactNode
  onClick: () => Promise<void>
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
