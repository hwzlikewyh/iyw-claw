"use client"

import { useConversationPresentation } from "./conversation-presentation-context"
import { isTaskHistory } from "./runtime-task-presentation"
import { RuntimeTaskDetailDialog } from "./runtime-task-detail-dialog"

export function RuntimeTaskDetailHost({ active = true }: { active?: boolean }) {
  const state = useConversationPresentation()
  const inspection = state?.inspection
  if (!active || !state || !inspection) return null
  const { task, ...actions } = inspection
  const current = state.tasks.find((item) => item.id === task.id)
  const selected =
    current ??
    (isTaskHistory(task) ? task : { ...task, status: "unknown", actions: [] })
  return (
    <RuntimeTaskDetailDialog
      {...actions}
      task={selected}
      current={current != null}
      pending={state.pending}
      onControl={state.control}
      onClose={state.closeInspection}
    />
  )
}
