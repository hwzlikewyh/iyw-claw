"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { useRuntimeTasks } from "@/hooks/use-runtime-tasks"
import { isTerminalTask, type RuntimeTask } from "@/lib/runtime-tasks"
import { useRuntimeTaskConsumption } from "@/hooks/use-runtime-task-consumption"
import { SessionTaskPoints } from "./runtime-task-points"
import { invalidateConversationUsage } from "@/lib/conversation-usage-refresh"
import { RuntimeTaskRow } from "./runtime-task-row"

interface PanelProps {
  connectionId: string | null
  conversationId: number | null
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
}

export function RuntimeTaskPanel(props: PanelProps) {
  const { connectionId, onMessage, onCancelLoop } = props
  const conversationId =
    props.conversationId != null && props.conversationId > 0
      ? props.conversationId
      : null
  const t = useTranslations("RuntimeTasks")
  const state = useRuntimeTasks(connectionId, conversationId)
  const consumption = useRuntimeTaskConsumption(
    state.tasks.length > 0 ? conversationId : null
  )
  const [showHistory, setShowHistory] = useState(false)
  if (state.tasks.length === 0 && !state.error) return null
  return (
    <section
      className="mx-3 mb-2 rounded-xl border bg-muted/25 p-2 text-xs"
      aria-label={t("title")}
    >
      <TaskPanelHeader
        state={state}
        conversationId={conversationId}
        showHistory={showHistory}
        onToggle={() => setShowHistory(!showHistory)}
      />
      <SessionTaskPoints consumption={consumption} />
      {state.error && (
        <p role="alert" className="break-words px-1 py-2 text-destructive">
          {state.error}
        </p>
      )}
      <div className="max-h-72 space-y-1 overflow-y-auto">
        {visibleTasks(state.tasks, showHistory).map((task) => (
          <RuntimeTaskRow
            key={task.id}
            task={task}
            parentTitle={
              state.tasks.find(
                (parent) =>
                  parent.id === task.parent_id ||
                  parent.native_id === task.parent_id
              )?.title
            }
            consumption={consumption}
            pending={state.pending}
            onControl={state.control}
            onMessage={onMessage}
            onCancelLoop={connectionId ? onCancelLoop : undefined}
            connectionId={connectionId}
          />
        ))}
      </div>
    </section>
  )
}

function visibleTasks(tasks: RuntimeTask[], history: boolean) {
  return history
    ? tasks
    : tasks.filter(
        (task) => !isTerminalTask(task.status) || task.status === "failed"
      )
}

function TaskPanelHeader({
  state,
  conversationId,
  showHistory,
  onToggle,
}: {
  state: ReturnType<typeof useRuntimeTasks>
  conversationId: number | null
  showHistory: boolean
  onToggle: () => void
}) {
  const t = useTranslations("RuntimeTasks")
  const count = state.tasks.filter(
    (task) =>
      !isTerminalTask(task.status) &&
      task.status !== "unknown" &&
      !task.background?.ambient
  ).length
  return (
    <div className="flex flex-wrap items-center justify-between gap-2 px-1">
      <span className="font-medium">{t("active", { count })}</span>
      <div className="flex items-center gap-1">
        <Button type="button" variant="ghost" size="sm" onClick={onToggle}>
          {t(showHistory ? "hideHistory" : "history")}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={t("refresh")}
          onClick={() => {
            void state.refresh()
            if (conversationId != null)
              invalidateConversationUsage(conversationId, {
                settled: false,
                reset: false,
              })
          }}
        >
          <RefreshCw className="size-3.5" />
        </Button>
      </div>
    </div>
  )
}
