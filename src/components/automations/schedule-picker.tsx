"use client"

import { useState } from "react"
import { CalendarDays, Clock3, Globe } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { describeCron } from "@/lib/cron-humanize"
import { cn } from "@/lib/utils"

type ScheduleKind = "daily" | "weekdays" | "weekends" | "weekly" | "monthly"

const SCHEDULE_KINDS = [
  ["daily", "cronFreqDaily"],
  ["weekdays", "cronFreqWeekdays"],
  ["weekends", "cronFreqWeekends"],
  ["weekly", "cronFreqWeekly"],
  ["monthly", "cronFreqMonthly"],
] as const
const WEEKDAYS = [1, 2, 3, 4, 5, 6, 0]
const DOW_KEYS = [
  "dow0",
  "dow1",
  "dow2",
  "dow3",
  "dow4",
  "dow5",
  "dow6",
] as const

interface ScheduleState {
  kind: ScheduleKind | "legacy"
  time: string
  weekdays: number[]
  dayOfMonth: number
}

function pad(value: number): string {
  return String(value).padStart(2, "0")
}

function initialState(cron: string): ScheduleState {
  const descriptor = describeCron(cron)
  const timed = "hour" in descriptor
  const time = timed
    ? `${pad(descriptor.hour)}:${pad(descriptor.minute)}`
    : "09:00"
  if (descriptor.kind === "weekly") {
    return { kind: "weekly", time, weekdays: descriptor.dows, dayOfMonth: 1 }
  }
  if (descriptor.kind === "monthly") {
    return { kind: "monthly", time, weekdays: [1], dayOfMonth: descriptor.dom }
  }
  if (["daily", "weekdays", "weekends"].includes(descriptor.kind)) {
    return {
      kind: descriptor.kind as ScheduleKind,
      time,
      weekdays: [1],
      dayOfMonth: 1,
    }
  }
  return { kind: "legacy", time, weekdays: [1], dayOfMonth: 1 }
}

function buildCron(state: ScheduleState): string {
  const [hour, minute] = state.time.split(":").map(Number)
  if (!Number.isInteger(hour) || !Number.isInteger(minute)) return ""
  const prefix = `${minute} ${hour}`
  if (state.kind === "daily") return `${prefix} * * *`
  if (state.kind === "weekdays") return `${prefix} * * 1-5`
  if (state.kind === "weekends") return `${prefix} * * 0,6`
  if (state.kind === "weekly") {
    return `${prefix} * * ${[...state.weekdays].sort((a, b) => a - b).join(",")}`
  }
  if (state.kind === "monthly") return `${prefix} ${state.dayOfMonth} * *`
  return ""
}

export function SchedulePicker({
  initialCron,
  timezone,
  nextRun,
  onChange,
}: {
  initialCron: string
  timezone: string
  nextRun: string | null
  onChange: (cron: string) => void
}) {
  const t = useTranslations("Automations")
  const [state, setState] = useState(() => initialState(initialCron))

  const update = (changes: Partial<ScheduleState>) => {
    const next = { ...state, ...changes }
    setState(next)
    onChange(buildCron(next))
  }

  const toggleWeekday = (day: number) => {
    const selected = state.weekdays.includes(day)
    if (selected && state.weekdays.length === 1) return
    update({
      weekdays: selected
        ? state.weekdays.filter((value) => value !== day)
        : [...state.weekdays, day],
    })
  }

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-border bg-card/40 p-3">
      <div
        className="flex flex-wrap gap-1.5"
        role="group"
        aria-label={t("cronFreqLabel")}
      >
        {SCHEDULE_KINDS.map(([kind, label]) => (
          <Button
            key={kind}
            type="button"
            size="sm"
            variant={state.kind === kind ? "default" : "outline"}
            onClick={() => update({ kind })}
          >
            {t(label)}
          </Button>
        ))}
      </div>

      {state.kind === "legacy" ? (
        <p className="text-xs text-muted-foreground">{t("scheduleLegacy")}</p>
      ) : null}

      {state.kind === "weekly" ? (
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-medium text-muted-foreground">
            {t("cronDowLabel")}
          </span>
          <div
            className="grid grid-cols-7 gap-1"
            role="group"
            aria-label={t("cronDowLabel")}
          >
            {WEEKDAYS.map((day) => (
              <Button
                key={day}
                type="button"
                size="sm"
                variant={state.weekdays.includes(day) ? "default" : "outline"}
                className="min-w-0 px-1"
                aria-pressed={state.weekdays.includes(day)}
                onClick={() => toggleWeekday(day)}
              >
                {t(DOW_KEYS[day])}
              </Button>
            ))}
          </div>
        </div>
      ) : null}

      {state.kind === "monthly" ? (
        <div className="flex flex-col gap-1.5">
          <span className="inline-flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            <CalendarDays className="size-3.5" aria-hidden="true" />
            {t("cronDomLabel")}
          </span>
          <div
            className="grid grid-cols-7 gap-1"
            role="group"
            aria-label={t("cronDomLabel")}
          >
            {Array.from({ length: 31 }, (_, index) => index + 1).map((day) => (
              <Button
                key={day}
                type="button"
                size="icon-sm"
                variant={state.dayOfMonth === day ? "default" : "ghost"}
                className={cn(
                  "size-8",
                  state.dayOfMonth !== day && "border border-border"
                )}
                aria-pressed={state.dayOfMonth === day}
                onClick={() => update({ dayOfMonth: day })}
              >
                {day}
              </Button>
            ))}
          </div>
        </div>
      ) : null}

      <div className="flex flex-wrap items-end gap-3">
        {state.kind !== "legacy" ? (
          <label className="flex min-w-36 flex-1 flex-col gap-1.5 text-xs font-medium text-muted-foreground">
            <span className="inline-flex items-center gap-1.5">
              <Clock3 className="size-3.5" aria-hidden="true" />
              {t("cronTimeLabel")}
            </span>
            <Input
              type="time"
              step={60}
              value={state.time}
              onChange={(event) => update({ time: event.target.value })}
            />
          </label>
        ) : null}
        <div className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
          <span>
            {t("nextRun")}: {nextRun ? new Date(nextRun).toLocaleString() : "-"}
          </span>
          <span
            className="inline-flex items-center gap-1"
            title={t("timezone")}
          >
            <Globe className="size-3 shrink-0" aria-hidden="true" />
            <span className="truncate font-mono">{timezone}</span>
          </span>
        </div>
      </div>
    </div>
  )
}
