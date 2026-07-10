"use client"

import { BarChart3 } from "lucide-react"
import { useTranslations } from "next-intl"

import { formatTokenCount } from "@/lib/token-format"
import {
  usageTotal,
  type UsageDailyRow,
  type UsageDashboardStats,
  type UsageModelRow,
} from "@/lib/usage-stats"

export interface UsageSnapshot {
  stats: UsageDashboardStats
}

function formatPercent(value: number): string {
  return `${(value * 100).toFixed(1)}%`
}

function formatCost(): string {
  return "$0.00"
}

function chartValue(row: UsageDailyRow | UsageModelRow): number {
  return row.input + row.output + row.cacheRead
}

function LegendDot({ className }: { className: string }) {
  return <span className={`h-2.5 w-2.5 rounded-sm ${className}`} />
}

function UsageLegend() {
  const t = useTranslations("UsageSettings")
  return (
    <div className="flex flex-wrap items-center gap-4 text-xs text-muted-foreground">
      <span className="inline-flex items-center gap-1.5">
        <LegendDot className="bg-indigo-500" />
        {t("legend.input")}
      </span>
      <span className="inline-flex items-center gap-1.5">
        <LegendDot className="bg-teal-500" />
        {t("legend.output")}
      </span>
      <span className="inline-flex items-center gap-1.5">
        <LegendDot className="bg-amber-500" />
        {t("legend.cacheRead")}
      </span>
    </div>
  )
}

function SegmentBar({ row, max }: { row: UsageModelRow; max: number }) {
  const total = chartValue(row)
  const width = max > 0 ? Math.max(2, (total / max) * 100) : 0
  const segments = [
    { value: row.input, className: "bg-indigo-500" },
    { value: row.output, className: "bg-teal-500" },
    { value: row.cacheRead, className: "bg-amber-500" },
  ].filter((segment) => segment.value > 0)

  return (
    <div className="h-5 overflow-hidden rounded-sm bg-muted/50">
      {total > 0 && (
        <div className="flex h-full" style={{ width: `${width}%` }}>
          {segments.map((segment) => (
            <div
              key={segment.className}
              className={segment.className}
              style={{ width: `${(segment.value / total) * 100}%` }}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function DailyBar({ row, max }: { row: UsageDailyRow; max: number }) {
  const total = chartValue(row)
  const height = max > 0 ? (total / max) * 100 : 0
  const segments = [
    { value: row.input, className: "bg-indigo-500" },
    { value: row.output, className: "bg-teal-500" },
    { value: row.cacheRead, className: "bg-amber-500" },
  ].filter((segment) => segment.value > 0)

  return (
    <div className="relative h-44 min-w-4 flex-1 rounded-sm bg-muted/40">
      {total > 0 && (
        <div
          className="absolute bottom-0 flex w-full flex-col-reverse overflow-hidden rounded-sm"
          style={{ height: `${Math.max(2, height)}%` }}
          title={`${row.date}: ${formatTokenCount(total)}`}
        >
          {segments.map((segment) => (
            <div
              key={segment.className}
              className={segment.className}
              style={{ height: `${(segment.value / total) * 100}%` }}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function StatCard({
  label,
  value,
  hint,
}: {
  label: string
  value: string
  hint: string
}) {
  return (
    <section className="rounded-lg border bg-card p-4">
      <p className="text-xs text-muted-foreground">{label}</p>
      <div className="mt-2 text-2xl font-semibold tracking-normal">{value}</div>
      <p className="mt-1 text-xs text-muted-foreground">{hint}</p>
    </section>
  )
}

export function UsageSummary({ snapshot }: { snapshot: UsageSnapshot }) {
  const t = useTranslations("UsageSettings")
  const { stats } = snapshot

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      <StatCard
        label={t("cards.totalTokens")}
        value={formatTokenCount(stats.totalTokens)}
        hint={t("cards.totalTokensHint", {
          input: formatTokenCount(stats.total.input),
          output: formatTokenCount(stats.total.output),
        })}
      />
      <StatCard
        label={t("cards.sessions")}
        value={stats.sessionCount.toLocaleString()}
        hint={t("cards.sessionsHint", {
          average: stats.averageDailySessions.toFixed(1),
        })}
      />
      <StatCard
        label={t("cards.estimatedCost")}
        value={formatCost()}
        hint={t("cards.estimatedCostHint")}
      />
      <StatCard
        label={t("cards.cacheHitRate")}
        value={formatPercent(stats.cacheHitRate)}
        hint={t("cards.cacheHitRateHint", {
          tokens: formatTokenCount(stats.total.cacheRead),
        })}
      />
    </div>
  )
}

export function ModelDistribution({ rows }: { rows: UsageModelRow[] }) {
  const t = useTranslations("UsageSettings")
  const max = Math.max(...rows.map(chartValue), 1)

  return (
    <section className="space-y-4 rounded-lg border bg-card p-4">
      <div className="space-y-3">
        <h2 className="text-sm font-semibold">{t("model.title")}</h2>
        <UsageLegend />
      </div>
      <div className="space-y-3">
        {rows.slice(0, 8).map((row) => (
          <div
            key={row.model}
            className="grid grid-cols-[minmax(8rem,12rem)_1fr_auto] items-center gap-3 text-xs"
          >
            <span className="truncate font-mono text-foreground">
              {row.model}
            </span>
            <SegmentBar row={row} max={max} />
            <span className="w-20 text-right tabular-nums text-muted-foreground">
              {formatTokenCount(chartValue(row))}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}

export function DailyUsage({ rows }: { rows: UsageDailyRow[] }) {
  const t = useTranslations("UsageSettings")
  const headers = [
    t("table.date"),
    t("table.input"),
    t("table.output"),
    t("table.cacheRead"),
    t("table.cacheWrite"),
    t("table.cacheHitRate"),
    t("table.sessions"),
    t("table.cost"),
  ]
  const visibleRows = rows
    .filter((row) => row.total > 0)
    .slice(-10)
    .reverse()
  const max = Math.max(...rows.map(chartValue), 1)
  const first = rows[0]?.date.slice(5) ?? ""
  const last = rows[rows.length - 1]?.date.slice(5) ?? ""

  return (
    <section className="space-y-4 rounded-lg border bg-card p-4">
      <h2 className="text-sm font-semibold">{t("daily.title")}</h2>
      <div className="flex items-end gap-1 overflow-hidden">
        {rows.map((row) => (
          <DailyBar key={row.date} row={row} max={max} />
        ))}
      </div>
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>{first}</span>
        <span>{last}</span>
      </div>
      <UsageLegend />
      <div className="overflow-x-auto">
        <table className="w-full min-w-[760px] text-xs">
          <thead className="text-muted-foreground">
            <tr className="border-b">
              {headers.map((label) => (
                <th
                  key={label}
                  className="py-2 text-right font-medium first:text-left"
                >
                  {label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visibleRows.map((row) => (
              <tr key={row.date} className="border-b last:border-0">
                <td className="py-2 font-mono">{row.date}</td>
                <td className="py-2 text-right tabular-nums">
                  {formatTokenCount(row.input)}
                </td>
                <td className="py-2 text-right tabular-nums">
                  {formatTokenCount(row.output)}
                </td>
                <td className="py-2 text-right tabular-nums">
                  {formatTokenCount(row.cacheRead)}
                </td>
                <td className="py-2 text-right tabular-nums">
                  {formatTokenCount(row.cacheWrite)}
                </td>
                <td className="py-2 text-right tabular-nums">
                  {formatPercent(row.cacheHitRate)}
                </td>
                <td className="py-2 text-right tabular-nums">{row.sessions}</td>
                <td className="py-2 text-right tabular-nums">{formatCost()}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}

export function UsageEmptyState() {
  const t = useTranslations("UsageSettings")
  return (
    <section className="flex min-h-52 flex-col items-center justify-center gap-3 rounded-lg border bg-card p-6 text-center">
      <BarChart3 className="h-8 w-8 text-muted-foreground" />
      <div className="space-y-1">
        <h2 className="text-sm font-semibold">{t("empty.title")}</h2>
        <p className="text-xs text-muted-foreground">
          {t("empty.description")}
        </p>
      </div>
    </section>
  )
}

export function isUsageSnapshotEmpty(snapshot: UsageSnapshot | null): boolean {
  return snapshot !== null && usageTotal(snapshot.stats.total) === 0
}
