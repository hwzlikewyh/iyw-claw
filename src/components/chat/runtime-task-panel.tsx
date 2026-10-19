"use client"

import { useState, type ReactNode } from "react"
import { ChevronUp, History, ListTodo, Radio } from "lucide-react"
import { useTranslations } from "next-intl"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import { useRuntimeTaskConsumption } from "@/hooks/use-runtime-task-consumption"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { useConversationActivityVisibility } from "@/components/message/conversation-activity-visibility"
import { useConversationPresentation } from "./conversation-presentation-context"
import {
  isTaskHistory,
  isListeningTask,
  taskControlsAvailable,
} from "./runtime-task-presentation"
import { RuntimeTaskRow } from "./runtime-task-row"
import { SessionTaskPoints } from "./runtime-task-points"

interface TaskActions {
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
}
interface PanelProps extends TaskActions {
  connectionId: string | null
  conversationId: number | null
}
interface PopoverProps extends TaskActions {
  children?: ReactNode
  historyOnly?: boolean
  monitorsOnly?: boolean
  onOpenChange?: (open: boolean) => void
}

export function RuntimeTaskPanel(props: PanelProps) {
  const state = useConversationPresentation()
  const { monitorVisible } = useConversationActivityVisibility()
  const [openedScope, setOpenedScope] = useState<string | null>(null)
  if (!state) return null
  const scope = JSON.stringify([state.connectionId, state.conversationId])
  const open = openedScope === scope
  const current = state.tasks.filter((task) => !isTaskHistory(task))
  if (!current.length && !state.error && !open) return null
  if (!open && !state.error && monitorVisible && current.every(isListeningTask))
    return null
  return (
    <div className="flex justify-end pb-1 text-xs">
      <RuntimeTaskPopover
        {...props}
        monitorsOnly={
          current.length > 0 &&
          current.every(isListeningTask) &&
          taskControlsAvailable(state.status, state.error)
        }
        onOpenChange={(shown) => setOpenedScope(shown ? scope : null)}
      />
    </div>
  )
}

export function RuntimeTaskPopover(props: PopoverProps) {
  const state = useConversationPresentation()
  if (!state || (!state.tasks.length && !state.error)) return null
  if (props.historyOnly && !state.tasks.some(isTaskHistory)) return null
  return (
    <TaskPopover key={state.contextKey ?? state.conversationId} {...props} />
  )
}

function TaskPopover(props: PopoverProps) {
  const state = useConversationPresentation()!
  const [open, setOpen] = useState(false)
  const [history, setHistory] = useState(false)
  const current = state.tasks.filter((task) => !isTaskHistory(task))
  const records = props.historyOnly || history || (open && !current.length)
  const tasks = records
    ? state.tasks.filter(isTaskHistory)
    : props.monitorsOnly
      ? current.filter(isListeningTask)
      : current
  return (
    <Popover
      open={open}
      onOpenChange={(shown) => {
        setOpen(shown)
        props.onOpenChange?.(shown)
      }}
    >
      <TaskTrigger {...props} count={current.length} />
      <PopoverContent
        side="top"
        align={props.monitorsOnly ? "start" : "end"}
        className="conversation-task-popover w-80 max-w-[calc(100vw-2rem)] gap-2 p-3"
      >
        <TaskPopoverHeader
          historyOnly={props.historyOnly || !current.length}
          records={Boolean(records)}
          onToggle={() => setHistory(!history)}
        />
        <TaskRows
          {...props}
          tasks={tasks}
          records={Boolean(records)}
          open={open}
        />
      </PopoverContent>
    </Popover>
  )
}

function TaskRows({
  tasks,
  records,
  open,
  ...actions
}: TaskActions & { tasks: RuntimeTask[]; records: boolean; open: boolean }) {
  const state = useConversationPresentation()!
  const p = useTranslations("ConversationPresentation")
  const consumption = useRuntimeTaskConsumption(
    open && state.conversationId != null && state.conversationId > 0
      ? state.conversationId
      : null
  )
  const controls = !records && taskControlsAvailable(state.status, state.error)
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
          connectionId={state.connectionId}
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
  const Icon = props.historyOnly
    ? History
    : props.monitorsOnly
      ? Radio
      : ListTodo
  const label = props.historyOnly
    ? p("history")
    : props.monitorsOnly
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
            {!props.historyOnly && (
              <ChevronUp aria-hidden="true" className="size-2.5" />
            )}
          </>
        )}
      </button>
    </PopoverTrigger>
  )
}

function TaskPopoverHeader({
  historyOnly,
  records,
  onToggle,
}: {
  historyOnly?: boolean
  records: boolean
  onToggle: () => void
}) {
  const state = useConversationPresentation()!
  const t = useTranslations("RuntimeTasks")
  const p = useTranslations("ConversationPresentation")
  return (
    <>
      <div className="flex items-center justify-between gap-2 text-xs">
        <strong>{records ? p("history") : t("title")}</strong>
        {!historyOnly && (
          <button
            type="button"
            onClick={onToggle}
            className="text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            {p(records ? "currentTasks" : "history")}
          </button>
        )}
      </div>
      {state.error && (
        <p role="alert" className="text-xs text-destructive break-words">
          {state.error}
        </p>
      )}
      {!taskControlsAvailable(state.status, state.error) && !records && (
        <p className="text-xs text-muted-foreground">{p("stateSyncing")}</p>
      )}
    </>
  )
}
