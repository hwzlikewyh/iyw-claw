"use client"

import { useCallback, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { CollabTranscriptDialog } from "@/components/message/collab-transcript-dialog"
import { useCollabTranscript } from "@/hooks/use-collab-transcript"
import {
  isTerminalTask,
  loadRuntimeTaskDetail,
  type RuntimeTask,
} from "@/lib/runtime-tasks"

export function RuntimeTaskTranscript({
  task,
  connectionId,
}: {
  task: RuntimeTask
  connectionId: string | null
}) {
  const t = useTranslations("RuntimeTasks")
  const [open, setOpen] = useState(false)
  if (!connectionId || !task.transcript_id) return null
  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        onClick={() => setOpen(true)}
      >
        {t("openChild")}
      </Button>
      {open && (
        <OpenTranscript
          task={task}
          connectionId={connectionId}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  )
}

function OpenTranscript({
  task,
  connectionId,
  onClose,
}: {
  task: RuntimeTask
  connectionId: string
  onClose: () => void
}) {
  const loader = useCallback(
    () => loadRuntimeTaskDetail(connectionId, task.id),
    [connectionId, task.id]
  )
  const record = useCollabTranscript(
    task.id,
    !isTerminalTask(task.status) && task.status !== "unknown",
    loader
  )
  return (
    <CollabTranscriptDialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
      name={task.title}
      detail={record.detail}
      loading={record.loading}
      error={record.error}
      onRefresh={record.refresh}
    />
  )
}
