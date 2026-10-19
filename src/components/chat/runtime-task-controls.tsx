"use client"

import { LoaderCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { RuntimeTask } from "@/lib/runtime-tasks"
import { isTaskHistory } from "./runtime-task-presentation"

export interface TaskActionProps {
  task: RuntimeTask
  pending: string | null
  onControl: (id: string, action: string, nativeId: string) => Promise<void>
  onMessage?: (task: RuntimeTask) => void
  onCancelLoop?: (task: RuntimeTask) => void
}

export function NativeTaskActions({
  task,
  pending,
  onControl,
}: TaskActionProps) {
  const t = useTranslations("RuntimeTasks")
  return task.actions.map((action) => (
    <Button
      key={action}
      type="button"
      size="sm"
      variant="ghost"
      disabled={pending !== null}
      onClick={() => void onControl(task.id, action, task.native_id)}
    >
      {pending === task.id && (
        <LoaderCircle
          aria-hidden="true"
          className="size-3 animate-spin motion-reduce:animate-none"
        />
      )}
      {t(
        `action.${action === "stop" && task.kind === "subagent" ? "interrupt" : action}`
      )}
    </Button>
  ))
}

export function RuntimeTaskActions(props: TaskActionProps) {
  const { task } = props
  const t = useTranslations("RuntimeTasks")
  if (isTaskHistory(task)) return null
  return (
    <div className="flex flex-wrap items-center gap-1 border-t pt-3">
      <NativeTaskActions {...props} />
      {props.onMessage && (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={() => props.onMessage?.(task)}
        >
          {t(task.kind === "subagent" ? "askParent" : "addMessage")}
        </Button>
      )}
      {task.kind === "loop" && props.onCancelLoop && (
        <>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => props.onCancelLoop?.(task)}
          >
            {t("requestCancelLoop")}
          </Button>
          <p className="basis-full text-xs text-muted-foreground">
            {t("cancelLoopHint")}
          </p>
        </>
      )}
      {task.status === "unknown" && (
        <p className="text-xs text-muted-foreground">{t("unknownHint")}</p>
      )}
    </div>
  )
}
