import type { TurnUsage } from "@/lib/types"

const POINT_DECIMAL_PLACES = 2
const MINIMUM_VISIBLE_POINTS = 0.01

function validPoints(value: unknown): value is string | number {
  return (
    (typeof value === "string" && /^\d+(?:\.\d{1,18})?$/.test(value)) ||
    (typeof value === "number" && Number.isFinite(value) && value >= 0)
  )
}

export function resolveUsagePoints(
  confirmed: unknown,
  estimate: unknown
): string | number | null {
  if (validPoints(confirmed)) return confirmed
  return validPoints(estimate) ? estimate : null
}

export function formatUsagePoints(points: number, locale?: string): string {
  if (points > 0 && points < MINIMUM_VISIBLE_POINTS) {
    return `<${MINIMUM_VISIBLE_POINTS.toLocaleString(locale)}`
  }
  return points.toLocaleString(locale, {
    maximumFractionDigits: POINT_DECIMAL_PLACES,
  })
}

export function formatLiveUsagePoints(
  points: string | number,
  locale?: string
): string {
  const value = Number(points)
  if (!Number.isFinite(value) || value < 0) return "—"
  return value.toLocaleString(locale, { maximumFractionDigits: 6 })
}

export function addUsagePoints(
  left: TurnUsage,
  right: TurnUsage
): number | null {
  const a = pointsOrZero(left)
  const b = pointsOrZero(right)
  return typeof a === "number" && typeof b === "number" ? a + b : null
}

function pointsOrZero(usage: TurnUsage): number | null | undefined {
  const total =
    usage.input_tokens +
    usage.output_tokens +
    usage.cache_read_input_tokens +
    usage.cache_creation_input_tokens
  return total === 0 ? 0 : usage.estimated_points
}
