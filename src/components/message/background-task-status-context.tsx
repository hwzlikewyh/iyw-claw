"use client"

import { createContext, useContext, useMemo, type ReactNode } from "react"
import {
  parseBackgroundTaskEnvelope,
  type BackgroundTaskBadge,
} from "@/lib/background-task"
import { terminalBackgroundBadge } from "@/lib/background-task-status"
import { parseBackgroundTaskMarker } from "@/lib/background-agent"
import type { MessageTurn } from "@/lib/types"
import type { ConversationTimelineTurn } from "@/stores/conversation-runtime-store"

const TaskStatuses = createContext<ReadonlyMap<string, BackgroundTaskBadge>>(
  new Map()
)

function collectTerminalStatuses(turns: MessageTurn[]) {
  const outcomes = new Map<string, BackgroundTaskBadge>()
  for (const turn of turns) {
    for (const block of turn.blocks) {
      if (block.type !== "tool_result") continue
      const envelope = parseBackgroundTaskEnvelope(block.output_preview)
      if (!envelope?.taskId || (envelope.kind === "stop" && block.is_error))
        continue
      const terminal = terminalBackgroundBadge(envelope.status)
      if (terminal)
        outcomes.set(
          envelope.taskId,
          terminal === "completed" && envelope.exitCode ? "failed" : terminal
        )
      else if (parseBackgroundTaskMarker(block.output_preview))
        outcomes.delete(envelope.taskId)
    }
  }
  return outcomes
}

export function BackgroundTaskStatusScope({
  turns,
  children,
}: {
  turns: ConversationTimelineTurn[]
  children: ReactNode
}) {
  const statuses = useMemo(
    () => collectTerminalStatuses(turns.map((entry) => entry.turn)),
    [turns]
  )
  return (
    <TaskStatuses.Provider value={statuses}>
      <div className="relative flex h-full min-h-0 flex-col">{children}</div>
    </TaskStatuses.Provider>
  )
}

export function useBackgroundTaskStatuses() {
  return useContext(TaskStatuses)
}
