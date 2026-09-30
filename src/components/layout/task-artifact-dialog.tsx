"use client"

import { useEffect } from "react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { TaskArtifactPreview } from "@/components/layout/task-artifact-preview"
import type { TaskArtifactInfo } from "@/lib/api"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useIsMobile } from "@/hooks/use-mobile"

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
    <TaskArtifactDialogContent
      artifact={artifact}
      open={open}
      onOpenChange={onOpenChange}
    />
  )
}

function TaskArtifactDialogContent({
  artifact,
  open,
  onOpenChange,
}: TaskArtifactDialogProps & { artifact: TaskArtifactInfo }) {
  const isMobile = useIsMobile()
  const { openArtifactPreview } = useAuxPanelContext()
  useEffect(() => {
    if (!isMobile && open) {
      openArtifactPreview(artifact)
      onOpenChange(false)
    }
  }, [artifact, isMobile, onOpenChange, open, openArtifactPreview])
  if (!isMobile) return null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="h-[min(42rem,calc(100dvh-2rem))] max-w-[min(64rem,calc(100vw-2rem))] overflow-hidden p-0 sm:max-w-[min(64rem,calc(100vw-2rem))]"
        closeButtonClassName="top-2 right-2"
      >
        <DialogTitle className="sr-only">{artifact.displayName}</DialogTitle>
        <DialogDescription className="sr-only">
          {artifact.displayName}
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
