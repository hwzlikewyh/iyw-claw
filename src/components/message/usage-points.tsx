"use client"

import { useLocale, useTranslations } from "next-intl"
import { formatUsagePoints } from "@/lib/usage-points"

export function UsagePointsValue({ points }: { points?: number | null }) {
  const locale = useLocale()
  const t = useTranslations("UsagePoints")
  const available =
    typeof points === "number" && Number.isFinite(points) && points >= 0
  return (
    <span
      className="tabular-nums"
      title={available ? t("estimateHint") : t("unavailableHint")}
    >
      {available
        ? t("value", { points: formatUsagePoints(points, locale) })
        : t("unavailable")}
    </span>
  )
}

export function UsagePointsRow({
  points,
  scope,
}: {
  points?: number | null
  scope: "turn" | "session"
}) {
  const t = useTranslations("UsagePoints")
  return (
    <div className="mt-1 flex items-center justify-between gap-4 border-t border-current/15 pt-1.5 text-xs font-medium">
      <span>{t(scope)}</span>
      <UsagePointsValue points={points} />
    </div>
  )
}
