"use client"

import { useEffect, useState } from "react"
import { CircleDashed } from "lucide-react"
import { useTranslations } from "next-intl"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { activityAge, latestObservedProcess } from "@/lib/session-activity"
import { formatElapsedLabel } from "@/lib/format-elapsed"

export function RuntimeBackgroundStatus({
  contextKey,
  inline = false,
}: {
  contextKey: string
  inline?: boolean
}) {
  const activity = useSessionActivity(contextKey)
  const t = useTranslations("Folder.chat.liveTurnStats")
  const [now, setNow] = useState(Date.now)
  const processes =
    activity?.snapshot.processes.filter(
      (p) =>
        (p.status === "running" || p.status === "unknown") &&
        (!inline || p.turn_generation !== activity?.snapshot.turn_generation)
    ) ?? []
  const latest = latestObservedProcess(processes)
  const observedAt = latest?.checked_at
  useEffect(() => {
    if (!observedAt) return
    const timer = setInterval(() => setNow(Date.now()), 1_000)
    return () => clearInterval(timer)
  }, [observedAt])
  if (!latest) return null
  const age = activityAge(activity, observedAt, now)
  return (
    <div
      className={
        inline
          ? "inline-flex flex-wrap items-center gap-1.5"
          : "flex min-h-8 flex-wrap items-center justify-center gap-1.5 px-4 py-1 text-xs text-muted-foreground"
      }
    >
      <CircleDashed className="size-3 shrink-0" />
      <span>{t("backgroundUnconfirmed", { count: processes.length })}</span>
      {age !== null && latest.status === "running" && (
        <span>
          {t("processObserved", { elapsed: formatElapsedLabel(age, t) })}
        </span>
      )}
    </div>
  )
}
