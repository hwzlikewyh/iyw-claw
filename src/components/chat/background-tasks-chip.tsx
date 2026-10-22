"use client"

import { CircleAlert, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { useConnection } from "@/hooks/use-connection"
import { RuntimeBackgroundStatus } from "./runtime-background-status"

interface BackgroundTasksChipProps {
  contextKey: string
  inline?: boolean
}

export function BackgroundTasksChip({
  contextKey,
  inline = false,
}: BackgroundTasksChipProps) {
  const t = useTranslations("Folder.chat.backgroundTasks")
  const { backgroundOutstanding, backgroundUncertain } =
    useConnection(contextKey)

  if (backgroundOutstanding <= 0 && !backgroundUncertain)
    return <RuntimeBackgroundStatus contextKey={contextKey} inline={inline} />

  const status = (
    <span className="inline-flex min-w-0 items-center gap-1 leading-none text-sky-700 dark:text-sky-300">
      {backgroundUncertain ? (
        <CircleAlert className="size-3 shrink-0" />
      ) : (
        <Loader2 className="size-3 shrink-0 animate-spin" />
      )}
      <span className="min-w-0 truncate">
        {backgroundUncertain
          ? t("unknown")
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
