"use client"

import { useSyncExternalStore, useCallback } from "react"
import { CircleAlert, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { useConnection } from "@/hooks/use-connection"
import { RuntimeBackgroundStatus } from "./runtime-background-status"
import { useConversationPresentation } from "./conversation-presentation-context"
import { isTaskHistory } from "./runtime-task-presentation"
import { useOptionalConnectionStore } from "@/contexts/acp-connections-context"

interface BackgroundTasksChipProps {
  contextKey: string
  inline?: boolean
}

export function BackgroundTasksChip({
  contextKey,
  inline = false,
}: BackgroundTasksChipProps) {
  const t = useTranslations("Folder.chat.backgroundTasks")
  const runtime = useConversationPresentation()
  const store = useOptionalConnectionStore()
  const subscribe = useCallback(
    (cb: () => void) => store?.subscribeKey(contextKey, cb) ?? (() => {}),
    [store, contextKey]
  )
  const snapshot = useCallback(
    () => store?.getConnection(contextKey)?.runtimeTaskActivity?.monitors ?? 0,
    [store, contextKey]
  )
  const monitors = useSyncExternalStore(subscribe, snapshot, () => 0)
  const { backgroundOutstanding, backgroundUncertain } =
    useConnection(contextKey)

  if (
    runtime?.contextKey === contextKey &&
    !runtime.error &&
    runtime.tasks.some(
      (task) =>
        !isTaskHistory(task) &&
        (task.kind === "monitor" || task.kind === "background")
    )
  )
    return null

  if (backgroundOutstanding <= 0 && !backgroundUncertain)
    return <RuntimeBackgroundStatus contextKey={contextKey} inline={inline} />

  const status = (
    <span className="inline-flex min-w-0 items-center gap-1 leading-none text-sky-700 dark:text-sky-300">
      {backgroundUncertain ? (
        <CircleAlert className="size-3 shrink-0" />
      ) : (
        <Loader2 className="size-3 shrink-0" />
      )}
      <span className="min-w-0 truncate">
        {backgroundUncertain
          ? t("unknown")
          : monitors > 0
            ? t("monitoring", { count: monitors })
            : t("running", { count: backgroundOutstanding })}
      </span>
    </span>
  )

  if (inline) {
    return (
      <>
        <span className="text-border leading-none">|</span>
        {status}
      </>
    )
  }

  return (
    <div className="flex min-h-8 flex-wrap items-center justify-center gap-x-3 gap-y-1 px-4 py-1 text-xs leading-none text-muted-foreground">
      {status}
    </div>
  )
}
