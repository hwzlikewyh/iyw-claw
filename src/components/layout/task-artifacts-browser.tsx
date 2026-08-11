"use client"

import { type ReactNode } from "react"

import { TaskArtifactPreview } from "@/components/layout/task-artifact-preview"
import {
  TaskArtifactsList,
  type TaskArtifactGroup,
} from "@/components/layout/task-artifacts-list"
import { Button } from "@/components/ui/button"
import type { TaskArtifactInfo } from "@/lib/api"
import { cn } from "@/lib/utils"

export interface TaskArtifactsBrowserSelection {
  selected: TaskArtifactInfo | null
  mobilePreviewOpen: boolean
  select: (item: TaskArtifactInfo) => void
  showList: () => void
}

export function TaskArtifactsBrowser({
  groups,
  selection,
  onRequestClose,
}: {
  groups: TaskArtifactGroup[]
  selection: TaskArtifactsBrowserSelection
  onRequestClose?: () => void
}) {
  return (
    <div className="grid min-h-0 flex-1 md:grid-cols-[minmax(15rem,18rem)_minmax(0,1fr)]">
      <div
        className={cn(
          "min-h-0 flex-col border-r",
          selection.mobilePreviewOpen ? "hidden md:flex" : "flex"
        )}
      >
        <TaskArtifactsList
          groups={groups}
          selectedId={selection.selected?.id}
          openOnDoubleClick
          onSelect={selection.select}
          onOpenWorkspace={onRequestClose}
        />
      </div>
      <TaskArtifactPreview
        artifact={selection.selected}
        className={cn(
          "h-full",
          selection.mobilePreviewOpen ? "grid" : "hidden md:grid"
        )}
        onBack={selection.showList}
        onOpenWorkspace={onRequestClose}
        onPreview={selection.select}
      />
    </div>
  )
}

export function TaskArtifactState({
  icon,
  text,
  action,
  onAction,
}: {
  icon: ReactNode
  text: string
  action?: string
  onAction?: () => void
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center text-muted-foreground">
      {icon}
      <p className="text-sm">{text}</p>
      {action && (
        <Button size="sm" variant="outline" onClick={onAction}>
          {action}
        </Button>
      )}
    </div>
  )
}
