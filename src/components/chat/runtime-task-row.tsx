"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, LoaderCircle } from "lucide-react"
import { Button } from "@/components/ui/button"
import { isTerminalTask, type RuntimeTask } from "@/lib/runtime-tasks"
import { cn } from "@/lib/utils"
import type { RuntimeConsumption } from "@/hooks/use-runtime-task-consumption"
import { TaskPoints } from "./runtime-task-points"
import { SubAgentSessionDialog } from "@/components/message/sub-agent-session-dialog"
import { RuntimeTaskTranscript } from "./runtime-task-transcript"

const STATUS_KEYS = {
  active: "status.active",
  running: "status.running",
  starting: "status.starting",
  waiting: "status.waiting",
  paused: "status.paused",
  blocked: "status.blocked",
  limited: "status.limited",
  usage_limited: "status.usage_limited",
  budget_limited: "status.budget_limited",
  usageLimited: "status.usageLimited",
  budgetLimited: "status.budgetLimited",
  complete: "status.complete",
  completed: "status.completed",
  failed: "status.failed",
  stopped: "status.stopped",
  cancelled: "status.cancelled",
  cleared: "status.cleared",
  unknown: "status.unknown",
} as const

interface RowProps {
  parentTitle?: string
  connectionId: string | null
  task: RuntimeTask
  pending: string | null
  onControl: (id: string, action: string, nativeId: string) => Promise<void>
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
  consumption?: RuntimeConsumption | null
}

export function RuntimeTaskRow(props: RowProps) {
  const { task, pending } = props
  const t = useTranslations("RuntimeTasks")
  const statusKey = STATUS_KEYS[task.status as keyof typeof STATUS_KEYS]
  const status = statusKey ? t(statusKey) : task.status
  return (
    <details className="group rounded-lg border bg-background/70 px-2 py-1.5">
      <summary className="flex cursor-pointer list-none items-center gap-2">
        <ChevronDown className="size-3.5 shrink-0 transition-transform group-open:rotate-180" />
        <span className="shrink-0 text-muted-foreground">
          {t(`kind.${task.kind}`)}
        </span>
        <span className="min-w-0 flex-1 truncate">{task.title}</span>
        <span
          className={cn(
            "shrink-0",
            task.status === "failed" && "text-destructive"
          )}
          role="status"
        >
          {pending === task.id ? t("processing") : status}
        </span>
      </summary>
      <div className="space-y-2 pt-2">
        <TaskMetadata task={task} parentTitle={props.parentTitle} />
        <TaskPoints task={task} consumption={props.consumption ?? null} />
        <ChildTaskConversation task={task} />
        {task.kind === "subagent" && (
          <RuntimeTaskTranscript
            task={task}
            connectionId={props.connectionId}
          />
        )}
        <TaskActions {...props} />
      </div>
    </details>
  )
}

function ChildTaskConversation({ task }: { task: RuntimeTask }) {
  const [open, setOpen] = useState(false)
  const t = useTranslations("RuntimeTasks")
  if (task.child_conversation_id == null) return null
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
      <SubAgentSessionDialog
        open={open}
        onOpenChange={setOpen}
        childConversationId={task.child_conversation_id}
        childConnectionId={task.child_connection_id}
        agentType={task.agent_type}
        kickoffTask={task.objective}
      />
    </>
  )
}

function TaskActions({
  task,
  pending,
  onControl,
  onMessage,
  onCancelLoop,
}: RowProps) {
  const t = useTranslations("RuntimeTasks")
  return (
    <div className="flex flex-wrap items-center gap-1 border-t pt-2">
      <NativeTaskActions task={task} pending={pending} onControl={onControl} />
      {onMessage && !isTerminalTask(task.status) && (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={() => onMessage(task)}
        >
          {t(task.kind === "subagent" ? "askParent" : "addMessage")}
        </Button>
      )}
      {task.kind === "loop" && !isTerminalTask(task.status) && onCancelLoop && (
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => onCancelLoop(task)}
        >
          {t("requestCancelLoop")}
        </Button>
      )}
      {task.kind === "loop" && !isTerminalTask(task.status) && (
        <p className="basis-full text-muted-foreground">
          {t("cancelLoopHint")}
        </p>
      )}
      {task.status === "unknown" && (
        <span className="text-muted-foreground">{t("unknownHint")}</span>
      )}
    </div>
  )
}

function TaskMetadata({
  task,
  parentTitle,
}: {
  task: RuntimeTask
  parentTitle?: string
}) {
  const t = useTranslations("RuntimeTasks")
  return (
    <>
      {parentTitle && (
        <p className="text-muted-foreground">
          {t("parentTask", { title: parentTitle })}
        </p>
      )}
      <p className="whitespace-pre-wrap break-words">
        {task.objective ?? task.title}
      </p>
      {task.model && (
        <p className="text-muted-foreground">
          {t("model")}: {task.model}
        </p>
      )}
      {task.schedule && (
        <p>
          {t("schedule")}: {task.schedule}
        </p>
      )}
      {task.next_run && (
        <p>
          {t("nextRun")}: {task.next_run}
        </p>
      )}
      {task.summary && (
        <p className="whitespace-pre-wrap break-words text-muted-foreground">
          {task.summary}
        </p>
      )}
      {task.output && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-2 font-sans">
          {task.output}
        </pre>
      )}
    </>
  )
}

function NativeTaskActions({
  task,
  pending,
  onControl,
}: Pick<RowProps, "task" | "pending" | "onControl">) {
  const t = useTranslations("RuntimeTasks")
  return (
    <>
      {task.actions.map((action) => (
        <Button
          key={action}
          type="button"
          size="sm"
          variant="outline"
          disabled={pending !== null}
          onClick={() => void onControl(task.id, action, task.native_id)}
        >
          {pending === task.id && (
            <LoaderCircle className="size-3 animate-spin" />
          )}
          {t(
            `action.${action === "stop" && task.kind === "subagent" ? "interrupt" : action}`
          )}
        </Button>
      ))}
    </>
  )
}
