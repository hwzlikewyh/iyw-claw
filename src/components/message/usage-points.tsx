"use client"

import { useLocale, useTranslations } from "next-intl"
import { formatUsagePoints, resolveUsagePoints } from "@/lib/usage-points"
import { formatPointDecimal } from "@/lib/point-decimal"
import type { ConfirmedConsumption } from "@/lib/types"

export function UsagePointsValue({
  points,
  consumption,
  estimate,
}: {
  points?: string | number | null
  consumption?: ConfirmedConsumption | null
  estimate?: number | null
}) {
  const locale = useLocale()
  const t = useTranslations("UsagePoints")
  const confirmed = points ?? consumption?.amount
  const value = resolveUsagePoints(confirmed, estimate)
  return (
    <span
      className="tabular-nums"
      title={value == null ? undefined : String(value)}
    >
      {value != null ? (
        <>
          {t("value", {
            points:
              typeof value === "string"
                ? formatPointDecimal(value, locale)
                : formatUsagePoints(value, locale),
          })}
        </>
      ) : (
        t("pending")
      )}
    </span>
  )
}

export function UsagePointsRow({
  points,
  scope,
  consumption,
  estimate,
}: {
  points?: string | number | null
  scope: "turn" | "session"
  consumption?: ConfirmedConsumption | null
  estimate?: number | null
}) {
  const t = useTranslations("UsagePoints")
  return (
    <div className="mt-1 flex items-center justify-between gap-4 border-t border-current/15 pt-1.5 text-xs font-medium">
      <span>{t(scope)}</span>
      <UsagePointsValue
        points={points}
        consumption={consumption}
        estimate={estimate}
      />
    </div>
  )
}
