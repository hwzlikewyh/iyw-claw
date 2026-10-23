"use client"

import { useState, type ReactNode } from "react"
import { Bot, ChevronUp, ListTodo, Radio, SquareTerminal } from "lucide-react"
import { useTranslations } from "next-intl"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import { useRuntimeTaskConsumption } from "@/hooks/use-runtime-task-consumption"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { useConversationPresentation } from "./conversation-presentation-context"
import {
  isTaskHistory,
  isListeningTask,
  taskControlsAvailable,
} from "./runtime-task-presentation"
import { RuntimeTaskRow } from "./runtime-task-row"
import { SessionTaskPoints } from "./runtime-task-points"
import { isPersistentTask } from "./persistent-task-strip"

interface TaskActions {
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
}
interface PanelProps extends TaskActions {
  connectionId: string | null
  conversationId: number | null
}
interface PopoverProps extends TaskActions {
  excludePersistent?: boolean
  children?: ReactNode
  monitorsOnly?: boolean
}

export function RuntimeTaskPanel(props: PanelProps) {
  const state = useConversationPresentation()
  if (!state) return null
  const current = currentTasks(state.tasks, true)
  if (!current.length && !state.error) return null
  return (
    <div className="flex justify-end pb-1 text-xs">
      <RuntimeTaskPopover
        {...props}
        excludePersistent
        monitorsOnly={
          current.length > 0 &&
          current.every(isListeningTask) &&
          taskControlsAvailable(state.status, state.error)
        }
      />
    </div>
  )
}

export function RuntimeTaskPopover(props: PopoverProps) {
  const state = useConversationPresentation()
  if (
    !state ||
    (!currentTasks(state.tasks, props.excludePersistent).length && !state.error)
  )
    return null
  return (
    <TaskPopover key={state.contextKey ?? state.conversationId} {...props} />
  )
}

function TaskPopover(props: PopoverProps) {
  const state = useConversationPresentation()!
  const [open, setOpen] = useState(false)
  const current = currentTasks(state.tasks, props.excludePersistent)
  const tasks = props.monitorsOnly ? current.filter(isListeningTask) : current
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <TaskTrigger {...props} count={current.length} />
      <PopoverContent
        side="top"
        align={props.monitorsOnly ? "start" : "end"}
        className="conversation-task-popover w-80 max-w-[calc(100vw-2rem)] gap-2 p-3"
      >
        <TaskPopoverHeader />
        <TaskRows
          {...props}
          tasks={tasks}
          open={open}
          onView={(task) => {
            state.inspectTask({
              task,
              onMessage: props.onMessage,
              onCancelLoop: props.onCancelLoop,
            })
            setOpen(false)
          }}
        />
      </PopoverContent>
    </Popover>
  )
}

function currentTasks(tasks: RuntimeTask[], excludePersistent = false) {
  return tasks.filter(
    (task) =>
      !isTaskHistory(task) && (!excludePersistent || !isPersistentTask(task))
  )
}

function TaskRows({
  tasks,
  open,
  onView,
  ...actions
}: TaskActions & {
  tasks: RuntimeTask[]
  open: boolean
  onView: (task: RuntimeTask) => void
}) {
  const state = useConversationPresentation()!
  const p = useTranslations("ConversationPresentation")
  const consumption = useRuntimeTaskConsumption(
    open && state.conversationId != null && state.conversationId > 0
      ? state.conversationId
      : null
  )
  const controls = taskControlsAvailable(state.status, state.error)
  return (
    <div className="max-h-[min(24rem,55vh)] space-y-1 overflow-y-auto overscroll-contain text-xs">
      {!tasks.length && (
        <p className="py-4 text-center text-muted-foreground">
          {p("emptyTasks")}
        </p>
      )}
      {tasks.map((task) => (
        <RuntimeTaskRow
          key={task.id}
          task={controls ? task : { ...task, actions: [] }}
          pending={state.pending}
          onControl={state.control}
          onView={onView}
          syncing={!controls}
          onMessage={controls ? actions.onMessage : undefined}
          onCancelLoop={controls ? actions.onCancelLoop : undefined}
        />
      ))}
      <TaskConsumptionDetails consumption={consumption} />
    </div>
  )
}

function TaskConsumptionDetails({
  consumption,
}: {
  consumption: ReturnType<typeof useRuntimeTaskConsumption>
}) {
  const t = useTranslations("RuntimeTasks")
  return (
    <details className="border-t pt-2 text-muted-foreground">
      <summary className="cursor-pointer">{t("sessionPoints")}</summary>
      <SessionTaskPoints consumption={consumption} />
    </details>
  )
}

function TaskTrigger(props: PopoverProps & { count: number }) {
  const state = useConversationPresentation()!
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  const agents = currentTasks(state.tasks, props.excludePersistent).filter(
    (task) => task.kind === "subagent" || task.kind === "delegation"
  ).length
  const tasks = currentTasks(state.tasks, props.excludePersistent)
  const commands = tasks.filter(
    (task) => task.background?.task_type === "terminal"
  ).length
  const background = tasks.filter(
    (task) =>
      (task.kind === "background" &&
        task.background?.task_type !== "terminal") ||
      task.kind === "monitor"
  ).length
  const Icon = props.monitorsOnly ? Radio : ListTodo
  const label = props.monitorsOnly
    ? t("monitoring")
    : state.error
      ? p("stateSyncing")
      : p("taskCount", { count: props.count })
  return (
    <PopoverTrigger asChild>
      <button
        type="button"
        className={
          "conversation-task-trigger" +
          (props.monitorsOnly ? " is-monitor" : "")
        }
      >
        {props.children ?? (
          <>
            <Icon aria-hidden="true" className="size-3" />
            <span>{label}</span>
            {agents > 0 && !props.monitorsOnly && (
              <span className="conversation-task-agent-count">
                <Bot aria-hidden="true" className="size-3.5" />
                {t("kind.subagent")} {agents}
              </span>
            )}
            {background > 0 && !props.monitorsOnly && (
              <span className="conversation-task-agent-count">
                <Radio aria-hidden="true" className="size-3.5" />
                {t("kind.background")} {background}
              </span>
            )}
            {commands > 0 && (
              <span className="conversation-task-agent-count">
                <SquareTerminal aria-hidden="true" className="size-3.5" />
                {t("longCommand")} {commands}
              </span>
            )}
            <ChevronUp aria-hidden="true" className="size-2.5" />
          </>
        )}
      </button>
    </PopoverTrigger>
  )
}

function TaskPopoverHeader() {
  const state = useConversationPresentation()!
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  return (
    <>
      <div className="flex items-center justify-between gap-2 text-xs">
        <strong>{t("title")}</strong>
      </div>
      {state.error && (
        <p role="alert" className="text-xs text-destructive break-words">
          {state.error}
        </p>
      )}
      {!taskControlsAvailable(state.status, state.error) && (
        <p className="text-xs text-muted-foreground">{p("stateSyncing")}</p>
      )}
    </>
  )
}
