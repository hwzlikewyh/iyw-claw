"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import type { RuntimeConsumption } from "@/hooks/use-runtime-task-consumption"
import { TaskPoints } from "./runtime-task-points"
import { RuntimeTaskTranscript } from "./runtime-task-transcript"
import { SubAgentSessionDialog } from "@/components/message/sub-agent-session-dialog"

export function RuntimeTaskProgress({ task }: { task: RuntimeTask }) {
  const p = useTranslations("ConversationPresentation")
  const t = useTranslations("RuntimeTasks")
  return (
    <section className="space-y-2">
      <h3 className="text-xs font-medium text-muted-foreground">
        {p("latestProgress")}
      </h3>
      <p className="whitespace-pre-wrap break-words text-sm leading-7">
        {task.summary ||
          (task.kind === "monitor" ? t("monitorHint") : p("noProgress"))}
      </p>
    </section>
  )
}

export function RuntimeTaskRecords({
  task,
  parentTitle,
  consumption,
}: {
  task: RuntimeTask
  parentTitle?: string
  consumption: RuntimeConsumption | null
}) {
  const p = useTranslations("ConversationPresentation")
  const t = useTranslations("RuntimeTasks")
  return (
    <details className="space-y-3 border-t pt-3 text-xs text-muted-foreground">
      <summary className="cursor-pointer py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        {p("technicalDetails")}
      </summary>
      {parentTitle && <p>{t("parentTask", { title: parentTitle })}</p>}
      {task.model && (
        <p>
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
      <MonitorMetadata task={task} />
      {task.background?.command && (
        <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-2">
          {task.background.command}
        </pre>
      )}
      {task.output && (
        <pre className="max-h-60 overflow-auto whitespace-pre-wrap break-words rounded bg-muted p-2 font-sans">
          {task.output}
        </pre>
      )}
      <TaskPoints task={task} consumption={consumption} />
    </details>
  )
}

function MonitorMetadata({ task }: { task: RuntimeTask }) {
  const t = useTranslations("RuntimeTasks")
  const background = task.background
  if (task.kind !== "monitor" || !background) return null
  if (background.persistent === true) return <p>{t("monitorPersistent")}</p>
  return background.timeout_ms != null && background.timeout_ms > 0 ? (
    <p>{t("monitorDuration", { seconds: background.timeout_ms / 1000 })}</p>
  ) : null
}

export function RuntimeTaskConversation({
  task,
  connectionId,
}: {
  task: RuntimeTask
  connectionId: string | null
}) {
  const [open, setOpen] = useState(false)
  const t = useTranslations("RuntimeTasks")
  if (task.child_conversation_id == null)
    return task.kind === "subagent" ? (
      <RuntimeTaskTranscript task={task} connectionId={connectionId} />
    ) : null
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
