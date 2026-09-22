"use client"

import { TaskArtifactPreview } from "@/components/layout/task-artifact-preview"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import type { TaskArtifactInfo } from "@/lib/api"

export function ResourcePreview({
  artifact,
  onClose,
}: {
  artifact: TaskArtifactInfo | null
  onClose: () => void
}) {
  const { openConversations } = useWorkbenchRoute()
  if (!artifact) return null
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        className="h-[min(48rem,calc(100dvh-2rem))] max-w-[min(72rem,calc(100vw-2rem))] overflow-hidden p-0 sm:max-w-[min(72rem,calc(100vw-2rem))]"
        closeButtonClassName="top-2 right-2"
      >
        <DialogTitle className="sr-only">{artifact.displayName}</DialogTitle>
        <DialogDescription className="sr-only">
          {artifact.displayName}
        </DialogDescription>
        <TaskArtifactPreview
          artifact={artifact}
          className="h-full"
          onOpenWorkspace={() => {
            onClose()
            openConversations()
          }}
        />
      </DialogContent>
    </Dialog>
  )
}
