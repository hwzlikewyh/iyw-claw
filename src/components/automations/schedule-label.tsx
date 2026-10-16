"use client"

import { useMemo } from "react"
import { useLocale, useTranslations } from "next-intl"
import { describeCron } from "@/lib/cron-humanize"

const DOW_KEYS = [
  "dow0",
  "dow1",
  "dow2",
  "dow3",
  "dow4",
  "dow5",
  "dow6",
] as const

/**
 * Render a cron expression as a localized human-readable cadence
 * ("Weekdays at 9:00 AM"). Unrecognized expressions fall back to the raw cron
 * in a monospace span. Time-of-day is formatted in the active locale.
 */
export function ScheduleLabel({
  cron,
  emphasizeTime = false,
}: {
  cron: string
  emphasizeTime?: boolean
}) {
  const t = useTranslations("Automations")
  const locale = useLocale()
  const d = useMemo(() => describeCron(cron), [cron])

  const time = (hour: number, minute: number) =>
    new Date(2000, 0, 1, hour, minute).toLocaleTimeString(locale, {
      hour: "numeric",
      minute: "2-digit",
    })

  const renderLabel = (label: string, displayTime: string) => {
    if (!emphasizeTime) return <>{label}</>
    const index = label.indexOf(displayTime)
    if (index < 0) return <>{label}</>
    return (
      <span className="inline-flex flex-wrap items-baseline gap-x-2 text-[11px] font-normal">
        <span>{label.slice(0, index).trim()}</span>
        <strong className="text-[22px] font-medium tracking-tight tabular-nums">
          {displayTime}
        </strong>
        {label.slice(index + displayTime.length).trim()}
      </span>
    )
  }

  switch (d.kind) {
    case "everyMinutes":
      return <>{t("schedEveryMinutes", { n: d.n })}</>
    case "hourly":
      return <>{t("schedHourly")}</>
    case "daily":
      return renderLabel(
        t("schedDaily", { time: time(d.hour, d.minute) }),
        time(d.hour, d.minute)
      )
    case "weekdays":
      return renderLabel(
        t("schedWeekdays", { time: time(d.hour, d.minute) }),
        time(d.hour, d.minute)
      )
    case "weekends":
      return renderLabel(
        t("schedWeekends", { time: time(d.hour, d.minute) }),
        time(d.hour, d.minute)
      )
    case "weekly":
      return renderLabel(
        t("schedWeekly", {
          day: d.dows.map((dow) => t(DOW_KEYS[dow])).join(t("daySeparator")),
          time: time(d.hour, d.minute),
        }),
        time(d.hour, d.minute)
      )
    case "monthly":
      return renderLabel(
        t("schedMonthly", { day: d.dom, time: time(d.hour, d.minute) }),
        time(d.hour, d.minute)
      )
    case "raw":
      return <>{t("schedLegacy")}</>
  }
}
