"use client"

import { useLocale, useTranslations } from "next-intl"
import { formatUsagePoints } from "@/lib/usage-points"
import { formatPointDecimal } from "@/lib/point-decimal"
import type { ConfirmedConsumption } from "@/lib/types"

export function UsagePointsValue({
  points,
  consumption,
}: {
  points?: string | number | null
  consumption?: ConfirmedConsumption | null
}) {
  const locale = useLocale()
  const t = useTranslations("UsagePoints")
  const available =
    (typeof points === "string" && /^\d+(?:\.\d{1,18})?$/.test(points)) ||
    (typeof points === "number" && Number.isFinite(points) && points >= 0)
  return (
    <span
      className="tabular-nums"
      title={available ? undefined : t("unavailableHint")}
    >
      {available ? (
        <>
          {t(consumption?.state === "partial" ? "partialValue" : "value", {
            points:
              typeof points === "string"
                ? formatPointDecimal(points, locale)
                : formatUsagePoints(points as number, locale),
          })}
        </>
      ) : (
        t(consumption?.state === "pending" ? "pending" : "unavailable")
      )}
    </span>
  )
}

export function UsagePointsRow({
  points,
  scope,
  consumption,
}: {
  points?: string | number | null
  scope: "turn" | "session"
  consumption?: ConfirmedConsumption | null
}) {
  const t = useTranslations("UsagePoints")
  return (
    <div className="mt-1 flex items-center justify-between gap-4 border-t border-current/15 pt-1.5 text-xs font-medium">
      <span>{t(scope)}</span>
      <UsagePointsValue points={points} consumption={consumption} />
    </div>
  )
}

export function UsagePointsHint({
  consumption,
}: {
  consumption?: ConfirmedConsumption | null
}) {
  const t = useTranslations("UsagePoints")
  const locale = useLocale()
  const state = consumption?.state ?? "unavailable"
  const compact = consumption?.compaction_points
  return (
    <span className="mt-1 text-[10px] opacity-70">
      {t(
        state === "confirmed"
          ? "confirmedHint"
          : state === "partial"
            ? "partialHint"
            : state === "pending"
              ? "pendingHint"
              : "unavailableHint"
      )}
      {compact && compact !== "0" && (
        <>
          {" "}
          {t("compactionPoints", {
            points: formatPointDecimal(compact, locale),
          })}
        </>
      )}
    </span>
  )
}
