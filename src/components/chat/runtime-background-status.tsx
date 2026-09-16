"use client"

import { CircleDashed } from "lucide-react"
import { useTranslations } from "next-intl"
import { useSessionActivity } from "@/hooks/use-session-activity"

export function RuntimeBackgroundStatus({
  contextKey,
  inline = false,
}: {
  contextKey: string
  inline?: boolean
}) {
  const activity = useSessionActivity(contextKey)
  const t = useTranslations("Folder.chat.backgroundTasks")
  const processes =
    activity?.snapshot.processes.filter(
      (p) => p.status === "running" || p.status === "unknown"
    ) ?? []
  if (processes.length === 0) return null
  return (
    <div
      className={
        inline
          ? "inline-flex flex-wrap items-center gap-1.5"
          : "flex min-h-8 flex-wrap items-center justify-center gap-1.5 px-4 py-1 text-xs text-muted-foreground"
      }
    >
      <CircleDashed className="size-3 shrink-0" />
      <span>
        {processes.some((p) => p.status === "unknown")
          ? t("unknown")
          : t("processesRunning", { count: processes.length })}
      </span>
    </div>
  )
}
