"use client"

import { useEffect } from "react"

import { useConversationPresentation } from "./conversation-presentation-context"
import { isTaskHistory } from "./runtime-task-presentation"
import { RuntimeTaskDetailDialog } from "./runtime-task-detail-dialog"

export function RuntimeTaskDetailHost({ active = true }: { active?: boolean }) {
  const state = useConversationPresentation()
  const inspection = state?.inspection
  const current = state?.tasks.find((item) => item.id === inspection?.task.id)
  const ended = Boolean(current && isTaskHistory(current))
  const close = state?.closeInspection
  useEffect(() => {
    if (ended) close?.()
  }, [ended, close])
  if (!active || !state || !inspection || ended) return null
  const { task, ...actions } = inspection
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
