"use client"

import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import { useConversationActivityVisibility } from "@/components/message/conversation-activity-visibility"
import { useRuntimeTaskConsumption } from "@/hooks/use-runtime-task-consumption"
import { useConversationPresentation } from "./conversation-presentation-context"
import {
  isTaskHistory,
  taskControlsAvailable,
} from "./runtime-task-presentation"
import { RuntimeTaskStatus } from "./runtime-task-row"
import {
  RuntimeTaskActions,
  type TaskActionProps,
} from "./runtime-task-controls"
import {
  RuntimeTaskConversation,
  RuntimeTaskProgress,
  RuntimeTaskRecords,
} from "./runtime-task-details"

interface DetailProps extends TaskActionProps {
  onClose: () => void
  current: boolean
}

export function RuntimeTaskDetailDialog(props: DetailProps) {
  const state = useConversationPresentation()!
  const { pageVisible } = useConversationActivityVisibility()
  const controls =
    props.current &&
    !isTaskHistory(props.task) &&
    taskControlsAvailable(state.status, state.error)
  const task = controls ? props.task : { ...props.task, actions: [] }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose()
      }}
    >
      <DialogContent
        data-paused={!pageVisible}
        className="conversation-task-detail max-w-lg gap-4 rounded-2xl"
      >
        <TaskDetailHeader task={task} />
        <TaskDetailProgress task={task} controls={controls} />
        <TaskRecordSection task={task} />
        <RuntimeTaskActions
          {...props}
          task={task}
          onMessage={controls ? props.onMessage : undefined}
          onCancelLoop={controls ? props.onCancelLoop : undefined}
        />
      </DialogContent>
    </Dialog>
  )
}

function TaskDetailHeader({ task }: { task: RuntimeTask }) {
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  const source = task.agent_type
    ? p("taskSource", {
        kind: t(`kind.${task.kind}`),
        agent: getAgentDisplayName(task.agent_type),
      })
    : t(`kind.${task.kind}`)
  return (
    <DialogHeader>
      <DialogTitle>{task.title}</DialogTitle>
      <DialogDescription>{source}</DialogDescription>
    </DialogHeader>
  )
}

function TaskDetailProgress({
  task,
  controls,
}: {
  task: RuntimeTask
  controls: boolean
}) {
  const state = useConversationPresentation()!
  const p = useTranslations("ConversationPresentation")
  return (
    <>
      <RuntimeTaskStatus
        task={task}
        pending={state.pending}
        syncing={!controls && !isTaskHistory(task)}
      />
      {isTaskHistory(task) && (
        <p className="text-xs text-muted-foreground">{p("archivedTask")}</p>
      )}
      {task.objective && task.objective !== task.title && (
        <p className="whitespace-pre-wrap break-words text-sm leading-7">
          {task.objective}
        </p>
      )}
      <RuntimeTaskProgress task={task} />
      {state.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {state.error}
        </p>
      )}
    </>
  )
}

function TaskRecordSection({ task }: { task: RuntimeTask }) {
  const state = useConversationPresentation()!
  const consumption = useRuntimeTaskConsumption(
    state.conversationId != null && state.conversationId > 0
      ? state.conversationId
      : null
  )
  const connectionId = taskControlsAvailable(state.status, state.error)
    ? state.connectionId
    : null
  const parentTitle = state.tasks.find(
    (parent) =>
      parent.id === task.parent_id || parent.native_id === task.parent_id
  )?.title
  return (
    <>
      <RuntimeTaskConversation task={task} connectionId={connectionId} />
      <RuntimeTaskRecords
        task={task}
        consumption={consumption}
        parentTitle={parentTitle}
      />
    </>
  )
}
