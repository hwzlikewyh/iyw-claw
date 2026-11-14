"use client"

import { useTranslations } from "next-intl"
import { consumptionBreakdown } from "@/lib/consumption-breakdown"
import type { ConfirmedConsumption } from "@/lib/types"
import { UsagePointsValue } from "./usage-points"

const PURPOSE_LABELS = {
  generation: "modelUsage",
  image: "imageUsage",
  image_generation: "imageGenerationUsage",
  image_editing: "imageEditingUsage",
  tool: "toolUsage",
  image_analysis: "imageAnalysisUsage",
  compaction: "compactionUsage",
} as const

export function ConsumptionBreakdown({
  consumption,
}: {
  consumption?: ConfirmedConsumption | null
}) {
  const t = useTranslations("UsagePoints")
  const rows = consumptionBreakdown(consumption)
  if (rows.length === 0) return null
  return (
    <section className="space-y-3 border-b pb-4 text-xs">
      <h3 className="font-medium">{t("breakdown")}</h3>
      <dl className="space-y-3">
        {rows.map((row) => (
          <div key={row.key} className="flex items-start justify-between gap-3">
            <dt className="min-w-0 text-muted-foreground">
              {t(
                PURPOSE_LABELS[row.purpose as keyof typeof PURPOSE_LABELS] ??
                  "otherUsage"
              )}
              {row.name && (
                <span className="mt-0.5 block break-words text-foreground">
                  {row.name}
                </span>
              )}
            </dt>
            <dd className="shrink-0 text-end font-medium tabular-nums">
              <UsagePointsValue consumption={row.consumption} />
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}
