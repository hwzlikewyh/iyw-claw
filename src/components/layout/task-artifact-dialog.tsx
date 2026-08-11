"use client"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { TaskArtifactPreview } from "@/components/layout/task-artifact-preview"
import type { TaskArtifactInfo } from "@/lib/api"

interface TaskArtifactDialogProps {
  artifact: TaskArtifactInfo | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function TaskArtifactDialog({
  artifact,
  open,
  onOpenChange,
}: TaskArtifactDialogProps) {
  if (!artifact) return null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="h-[min(42rem,calc(100dvh-2rem))] max-w-[min(64rem,calc(100vw-2rem))] overflow-hidden p-0 sm:max-w-[min(64rem,calc(100vw-2rem))]">
        <DialogTitle className="sr-only">{artifact.displayName}</DialogTitle>
        <DialogDescription className="sr-only">
          {artifact.path}
        </DialogDescription>
        <TaskArtifactPreview
          artifact={artifact}
          className="h-full"
          onOpenWorkspace={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}
