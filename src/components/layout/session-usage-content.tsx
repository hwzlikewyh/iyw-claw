"use client"

import { useLocale, useTranslations } from "next-intl"
import { ChevronRight, X } from "lucide-react"
import { UsagePointsValue } from "@/components/message/usage-points"
import { formatContextWindowPercent } from "@/lib/context-window"
import { formatTokenCount, formatTokenThousands } from "@/lib/token-format"
import { tokenValue, type SessionUsageData } from "@/lib/session-usage-display"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"

const CONTEXT_WARNING_PERCENT = 90

function TokenValue({
  value,
  compact = false,
}: {
  value: number | null | undefined
  compact?: boolean
}) {
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
      {tokens === null
        ? t("unreported")
        : compact
          ? formatTokenCount(tokens)
          : formatTokenThousands(tokens, locale)}
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
    <section className="space-y-2.5">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span className="text-2xl font-semibold tracking-tight tabular-nums">
          {formatContextWindowPercent(percent)}
        </span>
        <span className="text-[11px] text-muted-foreground">
          {t("usedShort")} <TokenValue value={data.contextUsed} compact /> /{" "}
          <TokenValue value={data.contextMax} compact />
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
              : "h-full bg-blue-500 dark:bg-blue-400"
          }
          style={{ width: `${progress}%` }}
        />
      </div>
      {percent === null && (
        <div className="text-[11px] text-muted-foreground">
          {t(`source.${data.contextSource}`)}
        </div>
      )}
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
  const availableRows = rows.filter(([, value]) => value != null && value > 0)
  if (!data.modelId && !data.model && availableRows.length === 0) return null
  return (
    <section className="border-t border-border pt-2">
      <h3 className="mb-1 font-medium">{t("modelLimits")}</h3>
      <dl>
        {(data.model?.name || data.modelId) && (
          <UsageRow label={t("model")}>
            <span className="break-all">
              {data.model?.name ?? data.modelId}
            </span>
          </UsageRow>
        )}
        {availableRows.map(([key, value]) => (
          <UsageRow key={key} label={t(key)}>
            <TokenValue value={value && value > 0 ? value : null} />
          </UsageRow>
        ))}
      </dl>
    </section>
  )
}

function CumulativeUsage({ data }: { data: SessionUsageData }) {
  const t = useTranslations("Folder.statusBar.tokens")
  const rows = data.rows.filter(
    (row) =>
      tokenValue(row.value) !== null &&
      ((row.key !== "cacheRead" && row.key !== "cacheWrite") || row.value !== 0)
  )
  return (
    <section className="border-t border-border pt-2">
      <h3 className="mb-1 font-medium">{t("cumulativeUsage")}</h3>
      <dl>
        {rows.map((row) => (
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
      {rows.length === 0 && (
        <p className="text-muted-foreground">{t("unreported")}</p>
      )}
    </section>
  )
}

function SessionPointsSummary({ data }: { data: SessionUsageData }) {
  const t = useTranslations("Folder.statusBar.tokens")
  return (
    <dl className="flex flex-wrap items-baseline justify-between gap-2 border-t border-border pt-3">
      <dt className="text-muted-foreground">{t("sessionCost")}</dt>
      <dd className="font-medium">
        <UsagePointsValue points={data.points} consumption={data.consumption} />
      </dd>
    </dl>
  )
}

export function SessionUsageContent({
  data,
  onClose,
}: {
  data: SessionUsageData
  onClose: () => void
}) {
  const t = useTranslations("Folder.statusBar.tokens")
  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <h3 className="font-medium">{t("contextUsage")}</h3>
        <button
          type="button"
          onClick={onClose}
          aria-label={t("close")}
          className="-m-1 inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
        >
          <X className="size-3.5" />
        </button>
      </div>
      <ContextUsage data={data} />
      <SessionPointsSummary data={data} />
      <Collapsible>
        <CollapsibleTrigger className="group inline-flex items-center gap-1 rounded-sm text-[11px] text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring">
          <ChevronRight className="size-3 group-data-[state=open]:rotate-90" />
          {t("usageDetails")}
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-3 space-y-3">
          <CumulativeUsage data={data} />
          <ModelLimits data={data} />
        </CollapsibleContent>
      </Collapsible>
    </>
  )
}
