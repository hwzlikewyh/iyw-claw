"use client"

import { useLocale, useTranslations } from "next-intl"
import { UsagePointsValue } from "@/components/message/usage-points"
import { formatContextWindowPercent } from "@/lib/context-window"
import { formatTokenThousands } from "@/lib/token-format"
import { tokenValue, type SessionUsageData } from "@/lib/session-usage-display"

const CONTEXT_WARNING_PERCENT = 90

function TokenValue({ value }: { value: number | null | undefined }) {
  const locale = useLocale()
  const t = useTranslations("Folder.statusBar.tokens")
  const tokens = tokenValue(value)
  return (
    <span
      className="shrink-0 tabular-nums"
      title={
        tokens === null ? undefined : `${tokens.toLocaleString(locale)} tokens`
      }
    >
      {tokens === null ? t("unreported") : formatTokenThousands(tokens, locale)}
    </span>
  )
}

function UsageRow({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <div className="flex items-start justify-between gap-4 py-0.5">
      <dt className="min-w-0 text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-right">{children}</dd>
    </div>
  )
}

function ContextUsage({ data }: { data: SessionUsageData }) {
  const t = useTranslations("Folder.statusBar.tokens")
  const percent = data.contextPercent
  const progress = percent === null ? 0 : Math.min(100, Math.max(0, percent))
  return (
    <section className="space-y-1.5">
      <div className="flex items-center justify-between gap-3 font-medium">
        <span>{t("contextWindow")}</span>
        <span className="tabular-nums">
          {formatContextWindowPercent(percent)}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={t("contextWindowUsageAria")}
        aria-valuenow={percent === null ? undefined : progress}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuetext={
          percent === null
            ? t("unreported")
            : formatContextWindowPercent(percent)
        }
        className="relative h-1.5 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={
            progress >= CONTEXT_WARNING_PERCENT
              ? "h-full bg-amber-500"
              : "h-full bg-foreground/65"
          }
          style={{ width: `${progress}%` }}
        />
      </div>
      <dl>
        <UsageRow label={t("usedMax")}>
          <TokenValue value={data.contextUsed} /> /{" "}
          <TokenValue value={data.contextMax} />
        </UsageRow>
      </dl>
      <div className="text-[11px] text-muted-foreground">
        {t(`source.${data.contextSource}`)}
      </div>
    </section>
  )
}

function ModelLimits({ data }: { data: SessionUsageData }) {
  const t = useTranslations("Folder.statusBar.tokens")
  const rows = [
    ["modelContext", data.model?.contextWindow],
    ["maxInput", data.model?.maxInputTokens],
    ["maxOutput", data.model?.maxOutputTokens],
  ] as const
  return (
    <section className="border-t border-border pt-2">
      <h3 className="mb-1 font-medium">{t("modelLimits")}</h3>
      <dl>
        <UsageRow label={t("model")}>
          <span className="break-all">
            {data.model?.name ?? data.modelId ?? t("unreported")}
          </span>
        </UsageRow>
        {rows.map(([key, value]) => (
          <UsageRow key={key} label={t(key)}>
            <TokenValue value={value && value > 0 ? value : null} />
          </UsageRow>
        ))}
      </dl>
    </section>
  )
}

function CompactionLimits({ data }: { data: SessionUsageData }) {
  const t = useTranslations("Folder.statusBar.tokens")
  const source = t(`thresholdSource.${data.thresholdSource}`)
  return (
    <section className="border-t border-border pt-2">
      <h3 className="mb-1 font-medium">{t("compaction")}</h3>
      <dl>
        <UsageRow label={t("targetThreshold")}>
          <TokenValue value={data.threshold} />
        </UsageRow>
        <UsageRow label={t("thresholdOrigin")}>{source}</UsageRow>
        <UsageRow label={t("hostBudget")}>
          <TokenValue value={data.hostThreshold} />
        </UsageRow>
        <UsageRow label={t("policy")}>
          {t(`mode.${data.compactionMode}`)}
        </UsageRow>
        <UsageRow label={t("nativeThreshold")}>{t("unreported")}</UsageRow>
        {data.configStale && (
          <UsageRow label={t("configState")}>
            <span className="text-amber-600 dark:text-amber-400">
              {t("reconnectRequired")}
            </span>
          </UsageRow>
        )}
        {data.thresholdReached && (
          <UsageRow label={t("budgetState")}>{t("targetReached")}</UsageRow>
        )}
      </dl>
    </section>
  )
}

function CumulativeUsage({ data }: { data: SessionUsageData }) {
  const t = useTranslations("Folder.statusBar.tokens")
  return (
    <section className="border-t border-border pt-2">
      <h3 className="mb-1 font-medium">{t("cumulativeUsage")}</h3>
      <dl>
        {data.rows.map((row) => (
          <div
            key={row.key}
            className={
              row.key === "total"
                ? "mt-1 border-t border-border pt-1 font-medium"
                : undefined
            }
          >
            <UsageRow label={t(row.key)}>
              <TokenValue value={row.value} />
            </UsageRow>
          </div>
        ))}
      </dl>
      <div className="mt-2 flex items-center justify-between gap-4 border-t border-border pt-2 font-medium">
        <span>{t("estimatedPoints")}</span>
        <UsagePointsValue points={data.points} />
      </div>
    </section>
  )
}

export function SessionUsageContent({ data }: { data: SessionUsageData }) {
  return (
    <>
      <ContextUsage data={data} />
      <ModelLimits data={data} />
      <CompactionLimits data={data} />
      <CumulativeUsage data={data} />
    </>
  )
}
