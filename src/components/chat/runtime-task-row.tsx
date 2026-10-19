"use client"

import { ListTodo, Radio } from "lucide-react"
import { useTranslations } from "next-intl"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import { cn } from "@/lib/utils"
import {
  NativeTaskActions,
  type TaskActionProps,
} from "./runtime-task-controls"

const STATUS_KEYS = new Set([
  "active",
  "running",
  "starting",
  "waiting",
  "paused",
  "blocked",
  "limited",
  "usage_limited",
  "budget_limited",
  "usageLimited",
  "budgetLimited",
  "complete",
  "completed",
  "failed",
  "stopped",
  "cancelled",
  "cleared",
  "unknown",
  "expired",
  "declined",
  "canceled",
])

export function RuntimeTaskStatus({
  task,
  pending,
  syncing = false,
}: {
  task: RuntimeTask
  pending: string | null
  syncing?: boolean
}) {
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  const key =
    task.kind === "monitor" && task.status === "running"
      ? "monitoring"
      : STATUS_KEYS.has(task.status)
        ? `status.${task.status}`
        : null
  return (
    <span
      role="status"
      className={cn(
        "shrink-0 text-[11px] text-muted-foreground",
        task.status === "failed" && "text-destructive"
      )}
    >
      {pending === task.id
        ? t("processing")
        : syncing
          ? p("stateSyncing")
          : key
            ? t(key as never)
            : task.status}
    </span>
  )
}

export function RuntimeTaskRow(
  props: TaskActionProps & {
    onView: (task: RuntimeTask) => void
    syncing?: boolean
  }
) {
  const { task } = props
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  const Icon = task.kind === "monitor" ? Radio : ListTodo
  return (
    <div className="conversation-task-row border-b py-3 last:border-0">
      <div className="flex min-w-0 items-start gap-2">
        <Icon
          aria-label={t(`kind.${task.kind}`)}
          className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
        />
        <span className="min-w-0 flex-1 break-words text-xs leading-5">
          {task.title}
        </span>
      </div>
      <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-2 ps-5">
        <RuntimeTaskStatus
          task={task}
          pending={props.pending}
          syncing={props.syncing}
        />
        <button
          type="button"
          onClick={() => props.onView(task)}
          className="ms-auto text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {p("viewProgress")}
        </button>
        <NativeTaskActions
          {...props}
          task={{
            ...task,
            actions: task.actions.filter((action) => action === "stop"),
          }}
        />
      </div>
    </div>
  )
}
