"use client"

import { Clock3, MoreHorizontal, RotateCw, Target } from "lucide-react"
import { useTranslations } from "next-intl"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import { useConversationPresentation } from "./conversation-presentation-context"
import { useConversationActivityVisibility } from "@/components/message/conversation-activity-visibility"
import {
  NativeTaskActions,
  type TaskActionProps,
} from "./runtime-task-controls"
import { RuntimeTaskStatus } from "./runtime-task-row"
import {
  isTaskHistory,
  taskControlsAvailable,
} from "./runtime-task-presentation"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

interface StripActions {
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
}
interface TaskStripProps {
  task: RuntimeTask
  tasks: RuntimeTask[]
  actions: StripActions
  syncing: boolean
  paused: boolean
}

export function isPersistentTask(task: RuntimeTask) {
  return task.kind === "goal" || task.kind === "loop"
}

export function PersistentTaskStrip(actions: StripActions) {
  const state = useConversationPresentation()
  const { pageVisible } = useConversationActivityVisibility()
  if (!state) return null
  const tasks = state.tasks
    .filter((task) => isPersistentTask(task) && !isTaskHistory(task))
    .sort(
      (a, b) =>
        Number(b.kind === "goal") - Number(a.kind === "goal") ||
        a.id.localeCompare(b.id)
    )
  const task = tasks[0]
  if (!task) return null
  const controls = taskControlsAvailable(state.status, state.error)
  return (
    <TaskStrip
      key={JSON.stringify([state.connectionId, state.conversationId, task.id])}
      task={controls ? task : { ...task, actions: [] }}
      tasks={tasks}
      actions={controls ? actions : {}}
      syncing={!controls}
      paused={!pageVisible}
    />
  )
}

function TaskStrip(props: TaskStripProps) {
  const { task, actions } = props
  const state = useConversationPresentation()!
  const Icon = task.kind === "loop" ? RotateCw : Target
  const view = () => state.inspectTask({ task, ...actions })
  return (
    <section
      className="conversation-persistent-task"
      data-paused={props.paused}
    >
      <Icon
        aria-hidden="true"
        className="mt-1 size-4 shrink-0 text-primary/70"
      />
      <div className="min-w-0 flex-1">
        <TaskStripStatus task={task} syncing={props.syncing} />
        <button
          type="button"
          onClick={view}
          className="mt-1 w-full break-words text-left text-sm leading-6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {task.objective || task.title}
        </button>
        {task.kind === "loop" && <ScheduleDetails task={task} />}
      </div>
      <TaskStripMenu {...props} onView={view} />
    </section>
  )
}

function TaskStripStatus({
  task,
  syncing,
}: {
  task: RuntimeTask
  syncing: boolean
}) {
  const state = useConversationPresentation()!
  const t = useTranslations("RuntimeTasks")
  const scheduled =
    task.kind === "loop" &&
    !syncing &&
    ["active", "running", "waiting"].includes(task.status)
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
      <span>{t(`kind.${task.kind}`)}</span>
      {scheduled && (
        <Clock3
          aria-hidden="true"
          className="conversation-schedule-mark size-3"
        />
      )}
      {scheduled && state.pending !== task.id ? (
        <span>{t("scheduleEnabled")}</span>
      ) : (
        <RuntimeTaskStatus
          task={task}
          pending={state.pending}
          syncing={syncing}
        />
      )}
    </div>
  )
}

function TaskStripMenu(props: TaskStripProps & { onView: () => void }) {
  const { task, actions, onView } = props
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  return (
    <div className="flex shrink-0 flex-wrap items-start gap-1">
      {actions.onMessage && (
        <button
          type="button"
          onClick={() => actions.onMessage?.(task)}
          className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {t("addMessage")}
        </button>
      )}
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={t("title")}
            className="rounded p-1 text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {props.tasks.length > 1 ? (
              p("taskCount", { count: props.tasks.length })
            ) : (
              <MoreHorizontal className="size-4" />
            )}
          </button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-64 gap-2 p-3">
          <TaskStripActions task={task} {...actions} onView={onView} />
          {props.tasks.length > 1 && (
            <OtherPersistentTasks tasks={props.tasks} actions={actions} />
          )}
        </PopoverContent>
      </Popover>
    </div>
  )
}

function OtherPersistentTasks({
  tasks,
  actions,
}: {
  tasks: RuntimeTask[]
  actions: StripActions
}) {
  const state = useConversationPresentation()!
  return (
    <div className="max-h-48 overflow-y-auto border-t pt-2">
      {tasks.map((task) => (
        <button
          key={task.id}
          type="button"
          onClick={() => state.inspectTask({ task, ...actions })}
          className="block w-full break-words rounded px-1 py-2 text-left text-xs hover:bg-muted"
        >
          {task.objective || task.title}
        </button>
      ))}
    </div>
  )
}

function ScheduleDetails({ task }: { task: RuntimeTask }) {
  const t = useTranslations("RuntimeTasks")
  return (
    <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
      {task.schedule && (
        <span>
          {t("schedule")}: {task.schedule}
        </span>
      )}
      {task.next_run && (
        <span>
          {t("nextRun")}: {task.next_run}
        </span>
      )}
    </div>
  )
}

function TaskStripActions({
  task,
  onView,
  ...actions
}: StripActions & {
  task: RuntimeTask
  onView: () => void
}) {
  const state = useConversationPresentation()!
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  const props: TaskActionProps = {
    task,
    pending: state.pending,
    onControl: state.control,
  }
  return (
    <>
      <button
        type="button"
        onClick={onView}
        className="rounded p-1 text-left text-xs hover:bg-muted"
      >
        {p("viewProgress")}
      </button>
      <div className="flex flex-wrap gap-1">
        <NativeTaskActions {...props} />
      </div>
      {task.kind === "loop" && actions.onCancelLoop && (
        <>
          <button
            type="button"
            onClick={() => actions.onCancelLoop?.(task)}
            className="rounded p-1 text-left text-xs hover:bg-muted"
          >
            {t("requestCancelLoop")}
          </button>
          <p className="text-xs text-muted-foreground">{t("cancelLoopHint")}</p>
        </>
      )}
      {task.status === "unknown" && (
        <p className="text-xs text-muted-foreground">{t("unknownHint")}</p>
      )}
    </>
  )
}
