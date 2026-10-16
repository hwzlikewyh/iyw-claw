"use client"

import { Check, CircleAlert, Loader2, Pause } from "lucide-react"
import { useTranslations } from "next-intl"
import { cn } from "@/lib/utils"

const statusKeys = {
  running: "statusRunning",
  succeeded: "statusSucceeded",
  failed: "statusFailed",
  cancelled: "statusCancelled",
  skipped: "statusSkipped",
} as const
const statusStyles: Record<string, string> = {
  running: "text-primary",
  succeeded: "text-emerald-600 dark:text-emerald-400",
  failed: "text-destructive",
  cancelled: "text-muted-foreground",
  skipped: "text-muted-foreground",
}

export function AutomationStatus({
  status,
  retry = false,
}: {
  status: string | null
  retry?: boolean
}) {
  const t = useTranslations("Automations")
  const key =
    retry && status === "succeeded"
      ? "retrySucceeded"
      : retry && status === "running"
        ? "retryRunning"
        : statusKeys[status as keyof typeof statusKeys]
  const Icon =
    status === "running"
      ? Loader2
      : status === "failed"
        ? CircleAlert
        : status === "succeeded"
          ? Check
          : Pause
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap text-[11px]",
        statusStyles[status ?? ""] ?? "text-muted-foreground"
      )}
    >
      <Icon
        className={cn("size-3", status === "running" && "animate-spin")}
        aria-hidden="true"
      />
      {key ? t(key) : t("noPreviousRuns")}
    </span>
  )
}

export function dateTime(
  value: string | null,
  locale: string,
  timezone?: string
) {
  if (!value || Number.isNaN(Date.parse(value))) return "—"
  return new Date(value).toLocaleString(locale, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: timezone,
  })
}

export function runDuration(start: string | null, end: string | null) {
  if (!start || !end) return "—"
  const seconds = Math.max(
    0,
    Math.round((Date.parse(end) - Date.parse(start)) / 1000)
  )
  if (!Number.isFinite(seconds)) return "—"
  const SECONDS_PER_MINUTE = 60
  const minutes = Math.floor(seconds / SECONDS_PER_MINUTE)
  return minutes
    ? `${minutes}m ${seconds % SECONDS_PER_MINUTE}s`
    : `${seconds}s`
}
