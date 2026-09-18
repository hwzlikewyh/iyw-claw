"use client"

import { useState } from "react"
import { useLocale, useTranslations } from "next-intl"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { formatTokenCount } from "@/lib/token-format"
import type { UsageDailyRow } from "@/lib/usage-stats"
import { cn } from "@/lib/utils"
import { formatUsagePoints } from "./usage-presentation"

type ChartMode = "points" | "tokens"
const MAX_AXIS_LABELS = 7
interface ChartBarProps {
  row: UsageDailyRow
  max: number
  mode: ChartMode
}
const CHART_SEGMENTS = [
  { key: "input", color: "bg-blue-500" },
  { key: "output", color: "bg-emerald-500" },
  { key: "cacheRead", color: "bg-amber-500" },
  { key: "cacheWrite", color: "bg-rose-500" },
] as const

function ChartBar({ row, max, mode }: ChartBarProps) {
  const t = useTranslations("UsageSettings")
  const locale = useLocale()
  const value = mode === "points" ? row.totalPoints : row.total
  const segments =
    mode === "points"
      ? [{ key: "totalPoints", color: "bg-primary", value }]
      : CHART_SEGMENTS.map((segment) => ({
          ...segment,
          value: row[segment.key],
        }))
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          tabIndex={0}
          role="img"
          aria-label={`${row.date}: ${value.toLocaleString(locale)} ${mode === "points" ? t("table.points") : "Token"}`}
          className="group relative flex h-36 min-w-0 flex-1 items-end justify-center rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <div className="absolute inset-0 bg-muted/20 opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100" />
          <div
            className="relative flex w-full max-w-12 flex-col-reverse overflow-hidden rounded-t-sm"
            style={{
              height: `${value > 0 ? Math.max(1, (value / max) * 100) : 0}%`,
            }}
          >
            {segments
              .filter((segment) => segment.value > 0)
              .map((segment) => (
                <div
                  key={segment.key}
                  className={segment.color}
                  style={{ height: `${(segment.value / value) * 100}%` }}
                />
              ))}
          </div>
        </div>
      </TooltipTrigger>
      <ChartTooltip row={row} />
    </Tooltip>
  )
}

function ChartTooltip({ row }: { row: UsageDailyRow }) {
  const t = useTranslations("UsageSettings")
  const locale = useLocale()
  return (
    <TooltipContent className="min-w-44 space-y-2 p-3">
      <div className="font-medium">{row.date}</div>
      <div className="flex justify-between gap-5">
        <span>{t("table.points")}</span>
        <span className="tabular-nums">
          {formatUsagePoints(row.totalPoints, locale)}
        </span>
      </div>
      {CHART_SEGMENTS.map(({ key }) => (
        <div key={key} className="flex justify-between gap-5">
          <span>{t(`table.${key}`)}</span>
          <span className="tabular-nums">{formatTokenCount(row[key])}</span>
        </div>
      ))}
    </TooltipContent>
  )
}

function ChartLegend() {
  const t = useTranslations("UsageSettings")
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-2 text-xs text-muted-foreground">
      {CHART_SEGMENTS.map(({ key, color }) => (
        <span key={key} className="inline-flex items-center gap-1.5">
          <span className={cn("size-2 rounded-sm", color)} />
          {t(`table.${key}`)}
        </span>
      ))}
    </div>
  )
}

function ChartModeControl({
  mode,
  setMode,
}: {
  mode: ChartMode
  setMode: (mode: ChartMode) => void
}) {
  const t = useTranslations("UsageSettings")
  return (
    <div
      className="inline-flex rounded-md border bg-muted/30 p-0.5"
      role="group"
      aria-label={t("chart.metric")}
    >
      {(["points", "tokens"] as const).map((value) => (
        <button
          key={value}
          type="button"
          aria-pressed={mode === value}
          onClick={() => setMode(value)}
          className={cn(
            "h-7 min-w-16 rounded px-3 text-xs transition-colors",
            mode === value
              ? "bg-background font-medium text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          {value === "points" ? t("table.points") : "Token"}
        </button>
      ))}
    </div>
  )
}

function ChartDates({ rows }: { rows: UsageDailyRow[] }) {
  const labelStep = Math.max(1, Math.ceil(rows.length / MAX_AXIS_LABELS))
  return (
    <div
      className="grid text-center text-[10px] tabular-nums text-muted-foreground"
      style={{
        gridTemplateColumns: `repeat(${rows.length}, minmax(0, 1fr))`,
      }}
    >
      {rows.map((row, index) => (
        <span key={row.date} className="relative h-4">
          {(index % labelStep === 0 && index < rows.length - labelStep / 2) ||
          index === rows.length - 1 ? (
            <span
              className={cn(
                "absolute whitespace-nowrap",
                rows.length <= MAX_AXIS_LABELS
                  ? "left-1/2 -translate-x-1/2"
                  : index === 0
                    ? "left-0"
                    : index === rows.length - 1
                      ? "right-0"
                      : "left-1/2 -translate-x-1/2"
              )}
            >
              {row.date.slice(5)}
            </span>
          ) : null}
        </span>
      ))}
    </div>
  )
}

export function UsageDailyChart({ rows }: { rows: UsageDailyRow[] }) {
  const t = useTranslations("UsageSettings")
  const [mode, setMode] = useState<ChartMode>("points")
  const max = Math.max(
    1,
    ...rows.map((row) => (mode === "points" ? row.totalPoints : row.total))
  )
  return (
    <section className="min-w-0 space-y-4 border-b pb-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-sm font-semibold">{t("daily.title")}</h2>
        <ChartModeControl mode={mode} setMode={setMode} />
      </div>
      <div className="space-y-2">
        <div className="flex justify-between text-[11px] tabular-nums text-muted-foreground">
          <span>{mode === "points" ? t("table.points") : "Token"}</span>
          <span>{formatTokenCount(max)}</span>
        </div>
        <div
          className="flex items-end border-b border-border/70 px-1"
          style={{ gap: rows.length > 30 ? "1px" : "4px" }}
        >
          {rows.map((row) => (
            <ChartBar key={row.date} row={row} max={max} mode={mode} />
          ))}
        </div>
        <ChartDates rows={rows} />
      </div>
      {mode === "tokens" && <ChartLegend />}
    </section>
  )
}
