"use client"

import { Coins, Timer } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { UsagePointsValue } from "@/components/message/usage-points"
import {
  formatContextWindowPercent,
  resolveContextWindowPercent,
} from "@/lib/context-window"
import type { DbConversationSummary, SessionStats } from "@/lib/types"
import { formatTokenThousands } from "@/lib/token-format"
import {
  formatSessionDuration,
  resolveSessionDurationMs,
} from "./session-details-data"

export function SessionUsageOverview({
  stats,
  summary,
}: {
  stats: SessionStats | null
  summary: DbConversationSummary
}) {
  const t = useTranslations("Folder.sessionDetails")
  const pointsT = useTranslations("UsagePoints")
  const locale = useLocale()
  const usage = stats?.total_usage
  const total =
    stats?.total_tokens ??
    (usage
      ? usage.input_tokens +
        usage.output_tokens +
        usage.cache_read_input_tokens +
        usage.cache_creation_input_tokens
      : null)
  const duration = resolveSessionDurationMs(summary, stats)
  return (
    <section className="grid grid-cols-2 gap-5 border-b py-5">
      <div className="min-w-0 space-y-2">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Coins className="size-3.5" />
          {pointsT("session")}
        </div>
        <div className="break-words text-2xl font-semibold text-primary">
          <UsagePointsValue points={usage?.estimated_points} />
        </div>
        <p className="text-[11px] text-muted-foreground">
          {pointsT("estimateShort")}
        </p>
      </div>
      <div className="min-w-0 space-y-2">
        <div className="text-xs text-muted-foreground">
          {t("totalTokens")} Token
        </div>
        <div className="break-words text-xl font-semibold tabular-nums">
          {total == null ? t("none") : formatTokenThousands(total, locale)}
        </div>
        <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
          <Timer className="size-3" />
          {t("duration")}{" "}
          {duration > 0 ? formatSessionDuration(duration) : t("none")}
        </div>
      </div>
    </section>
  )
}

export function SessionTokenBreakdown({
  stats,
}: {
  stats: SessionStats | null
}) {
  const t = useTranslations("Folder.sessionDetails")
  const locale = useLocale()
  const usage = stats?.total_usage
  const rows = [
    { key: "inputTokens", value: usage?.input_tokens, color: "bg-blue-500" },
    {
      key: "outputTokens",
      value: usage?.output_tokens,
      color: "bg-emerald-500",
    },
    {
      key: "cacheRead",
      value: usage?.cache_read_input_tokens,
      color: "bg-amber-500",
    },
    {
      key: "cacheWrite",
      value: usage?.cache_creation_input_tokens,
      color: "bg-rose-500",
    },
  ] as const
  return (
    <section className="space-y-3 border-b py-4">
      <h3 className="text-xs font-medium">{t("tokensHeading")}</h3>
      <dl className="grid grid-cols-2 gap-x-6 gap-y-4">
        {rows.map((row) => (
          <div key={row.key} className="min-w-0 space-y-1">
            <dt className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <span className={`size-1.5 rounded-full ${row.color}`} />
              {t(row.key)}
            </dt>
            <dd className="break-words text-sm font-medium tabular-nums">
              {row.value == null
                ? t("none")
                : formatTokenThousands(row.value, locale)}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

export function SessionContextUsage({ stats }: { stats: SessionStats | null }) {
  const t = useTranslations("Folder.sessionDetails")
  const locale = useLocale()
  const used = stats?.context_window_used_tokens
  const max = stats?.context_window_max_tokens
  const percent = resolveContextWindowPercent(
    stats?.context_window_usage_percent,
    used,
    max
  )
  if (used == null && max == null) return null
  return (
    <section className="space-y-2 border-b py-4">
      <div className="flex justify-between gap-3 text-xs">
        <h3 className="font-medium">{t("contextWindow")}</h3>
        <span className="tabular-nums">
          {formatContextWindowPercent(percent)}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={t("contextWindow")}
        aria-valuenow={percent ?? undefined}
        aria-valuemin={0}
        aria-valuemax={100}
        className="h-1.5 overflow-hidden rounded-sm bg-muted"
      >
        <div
          className="h-full bg-teal-500"
          style={{ width: `${percent ?? 0}%` }}
        />
      </div>
      <p className="text-right text-xs tabular-nums text-muted-foreground">
        {used == null ? t("none") : formatTokenThousands(used, locale)} /{" "}
        {max == null ? t("none") : formatTokenThousands(max, locale)}
      </p>
    </section>
  )
}
