"use client"

import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import type { TaskArtifactInfo } from "@/lib/api"

import { CurrentReplyArtifactsDialog } from "./current-reply-artifacts-dialog"

export function CurrentReplyArtifactDialogs({
  items,
  selected,
  allOpen,
  onSelectArtifact,
  onOpenAllChange,
}: {
  items: TaskArtifactInfo[]
  selected: TaskArtifactInfo | null
  allOpen: boolean
  onSelectArtifact: (item: TaskArtifactInfo | null) => void
  onOpenAllChange: (open: boolean) => void
}) {
  return (
    <>
      <TaskArtifactDialog
        artifact={selected}
        items={items}
        onSelect={onSelectArtifact}
        open={selected !== null}
        onOpenChange={(open) => !open && onSelectArtifact(null)}
      />
      {allOpen && (
        <CurrentReplyArtifactsDialog
          items={items}
          open
          onOpenChange={onOpenAllChange}
        />
      )}
    </>
  )
}
