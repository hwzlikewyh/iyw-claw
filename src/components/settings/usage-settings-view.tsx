"use client"

import { BarChart3 } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"

import { formatTokenCount } from "@/lib/token-format"
import {
  usageTotal,
  type UsageDashboardStats,
  type UsageModelRow,
  type UsageDailyRow,
} from "@/lib/usage-stats"
import { formatUsagePoints } from "./usage-presentation"

export interface UsageSnapshot {
  stats: UsageDashboardStats
}

function formatPercent(value: number): string {
  return `${(value * 100).toFixed(1)}%`
}

function SummaryMetric({
  label,
  value,
  hint,
}: {
  label: string
  value: string
  hint: string
}) {
  return (
    <div className="min-w-0 py-3 pe-3">
      <p className="text-xs text-muted-foreground">{label}</p>
      <div className="mt-2 break-all text-xl font-semibold tabular-nums tracking-normal">
        {value}
      </div>
      <p className="mt-1 text-xs text-muted-foreground">{hint}</p>
    </div>
  )
}

export function UsageSummary({
  snapshot,
  days,
}: {
  snapshot: UsageSnapshot
  days: number
}) {
  const t = useTranslations("UsageSettings")
  const locale = useLocale()
  const { stats } = snapshot

  return (
    <div className="grid grid-cols-2 gap-x-5 border-y @2xl:grid-cols-4">
      <SummaryMetric
        label={t("cards.pointsUsed")}
        value={formatUsagePoints(stats.totalPoints, locale)}
        hint={t("cards.pointsUsedHint")}
      />
      <SummaryMetric
        label={t("cards.totalTokens")}
        value={formatTokenCount(stats.totalTokens)}
        hint={t("cards.totalTokensHint", {
          input: formatTokenCount(stats.total.input),
          output: formatTokenCount(stats.total.output),
        })}
      />
      <SummaryMetric
        label={t("cards.sessions")}
        value={stats.sessionCount.toLocaleString()}
        hint={t("cards.sessionsHint", {
          average: (stats.sessionCount / days).toFixed(1),
        })}
      />
      <SummaryMetric
        label={t("cards.cacheHitRate")}
        value={formatPercent(stats.cacheHitRate)}
        hint={t("cards.cacheHitRateHint", {
          tokens: formatTokenCount(stats.total.cacheRead),
        })}
      />
    </div>
  )
}

export function UsageTodaySummary({ row }: { row: UsageDailyRow }) {
  const t = useTranslations("UsageSettings")
  const locale = useLocale()
  return (
    <section aria-label={t("today.title")} className="space-y-1 border-b pb-2">
      <h2 className="text-sm font-semibold">{t("today.title")}</h2>
      <div className="grid grid-cols-2 gap-x-5 @2xl:grid-cols-4">
        <SummaryMetric
          label={t("cards.pointsUsed")}
          value={formatUsagePoints(row.totalPoints, locale)}
          hint={t("cards.pointsUsedHint")}
        />
        <SummaryMetric
          label={t("cards.totalTokens")}
          value={formatTokenCount(row.total)}
          hint={t("cards.totalTokensHint", {
            input: formatTokenCount(row.input),
            output: formatTokenCount(row.output),
          })}
        />
        <SummaryMetric
          label={t("cards.sessions")}
          value={row.sessions.toLocaleString(locale)}
          hint={row.date}
        />
        <SummaryMetric
          label={t("cards.cacheHitRate")}
          value={formatPercent(row.cacheHitRate)}
          hint={t("cards.cacheHitRateHint", {
            tokens: formatTokenCount(row.cacheRead),
          })}
        />
      </div>
    </section>
  )
}

export function ModelDistribution({ rows }: { rows: UsageModelRow[] }) {
  const t = useTranslations("UsageSettings")
  const locale = useLocale()
  const sorted = [...rows].sort(
    (left, right) => right.totalPoints - left.totalPoints
  )
  const max = Math.max(...rows.map((row) => row.totalPoints), 1)

  return (
    <section className="min-w-0 space-y-3 border-t pt-5">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-semibold">{t("model.title")}</h2>
        <span className="text-xs text-muted-foreground">
          {t("table.points")}
        </span>
      </div>
      <div className="space-y-3">
        {sorted.map((row) => (
          <div
            key={row.modelAlias}
            className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 text-xs @xl:grid-cols-[minmax(8rem,1fr)_minmax(0,2fr)_auto]"
          >
            <span className="min-w-0 break-words font-medium text-foreground">
              {row.modelDisplayName}
            </span>
            <div className="hidden h-1.5 overflow-hidden rounded-sm bg-muted @xl:block">
              <div
                className="h-full rounded-sm bg-teal-500"
                style={{ width: `${(row.totalPoints / max) * 100}%` }}
              />
            </div>
            <span className="text-right font-medium tabular-nums">
              {formatUsagePoints(row.totalPoints, locale)}
            </span>
            <span className="text-muted-foreground">
              {formatTokenCount(row.total)} Token
            </span>
            <span className="text-right text-muted-foreground @xl:col-span-2">
              {row.sessions.toLocaleString(locale)} {t("table.sessions")}
            </span>
          </div>
        ))}
      </div>
    </section>
  )
}

export function UsageEmptyState() {
  const t = useTranslations("UsageSettings")
  return (
    <section className="flex min-h-52 flex-col items-center justify-center gap-3 p-6 text-center">
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
  return (
    snapshot !== null &&
    usageTotal(snapshot.stats.total) === 0 &&
    snapshot.stats.sessionCount === 0 &&
    snapshot.stats.totalPoints === 0
  )
}
