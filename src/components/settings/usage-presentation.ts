import type { UsageDailyRow } from "@/lib/usage-stats"

export const USAGE_DAY_OPTIONS = [7, 14, 30, 90] as const
export const DEFAULT_USAGE_DAYS = USAGE_DAY_OPTIONS[0]

export function formatUsagePoints(value: number, locale?: string): string {
  return value.toLocaleString(locale, { maximumFractionDigits: 4 })
}

export function usageCalendarDays(
  rows: UsageDailyRow[],
  days: number
): UsageDailyRow[] {
  const byDate = new Map(rows.map((row) => [row.date, row]))
  const today = new Date()
  today.setHours(12, 0, 0, 0)
  return Array.from({ length: days }, (_, index) => {
    const day = new Date(today)
    day.setDate(day.getDate() - days + index + 1)
    const date = [
      day.getFullYear(),
      String(day.getMonth() + 1).padStart(2, "0"),
      String(day.getDate()).padStart(2, "0"),
    ].join("-")
    return (
      byDate.get(date) ?? {
        date,
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        total: 0,
        totalPoints: 0,
        sessions: 0,
        cacheHitRate: 0,
      }
    )
  })
}
