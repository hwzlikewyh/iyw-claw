"use client"

import { CircleDashed } from "lucide-react"
import { useTranslations } from "next-intl"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { useVisibleNow } from "@/hooks/use-visible-now"
import { activityAge } from "@/lib/session-activity"
import type { ObservedSessionActivity } from "@/lib/session-activity"
import type { ProcessObservation } from "@/lib/types"

const MILLISECONDS_PER_SECOND = 1_000

function useLastOutputAge(
  activity: ObservedSessionActivity | null,
  processes: ProcessObservation[]
) {
  const now = useVisibleNow(processes.length > 0)
  const lastOutput = processes.reduce<string | null>((latest, process) => {
    const at = process.output_at
    return at && (!latest || Date.parse(at) > Date.parse(latest)) ? at : latest
  }, null)
  return activityAge(
    activity,
    lastOutput,
    Math.max(now, activity?.receivedAt ?? 0)
  )
}

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
  const outputAge = useLastOutputAge(activity, processes)
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
      {outputAge !== null && (
        <span>
          {t("lastOutputAgo", {
            seconds: Math.floor(outputAge / MILLISECONDS_PER_SECOND),
          })}
        </span>
      )}
    </div>
  )
}
