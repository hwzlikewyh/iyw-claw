import type { ConfirmedConsumption, ConsumptionStage } from "./types"
import { mergeConfirmedPoints } from "./point-decimal"

function purposeOf(row: ConsumptionStage): string {
  if (row.purpose) return row.purpose
  if (row.stage_key === "tool") return "tool"
  if (row.stage_key.startsWith("vision_")) return "image_analysis"
  return "generation"
}

export function consumptionBreakdown(
  consumption?: ConfirmedConsumption | null
) {
  const rows = new Map<
    string,
    {
      key: string
      purpose: string
      name: string | null
      consumption: ConfirmedConsumption
    }
  >()
  const unique = new Map<string, ConsumptionStage>()
  for (const record of consumption?.records ?? [])
    unique.set(JSON.stringify([record.request_id, record.stage_key]), record)
  for (const record of unique.values()) {
    const purpose = purposeOf(record)
    const name = record.display_name?.trim() || null
    const key = JSON.stringify([purpose, name])
    const value: ConfirmedConsumption = {
      amount: record.actual_points,
      state: record.actual_points == null ? "pending" : "confirmed",
      records: [record],
    }
    rows.set(key, {
      key,
      purpose,
      name,
      consumption: mergeConfirmedPoints(rows.get(key)?.consumption, value)!,
    })
  }
  return [...rows.values()]
}
