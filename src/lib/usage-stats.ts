import type { ConversationDetail, TurnUsage } from "@/lib/types"

export interface UsageBreakdown {
  input: number
  output: number
  cacheRead: number
  cacheWrite: number
}

export interface UsageModelRow extends UsageBreakdown {
  model: string
  sessions: number
  total: number
}

export interface UsageDailyRow extends UsageBreakdown {
  date: string
  sessions: number
  total: number
  cacheHitRate: number
}

export interface UsageDashboardStats {
  total: UsageBreakdown
  totalTokens: number
  sessionCount: number
  cacheHitRate: number
  averageDailySessions: number
  firstDate: string | null
  lastDate: string | null
  modelRows: UsageModelRow[]
  dailyRows: UsageDailyRow[]
}

interface UsageStatsOptions {
  dayCount?: number
  now?: Date
}

interface ResolvedUsageStatsOptions {
  dayCount: number
  now: Date
}

interface UsageAccumulator {
  total: UsageBreakdown
  models: Map<string, UsageModelRow>
  daily: Map<string, UsageDailyRow>
  sessionCount: number
  firstDate: string | null
  lastDate: string | null
}

interface DailyRowsInput {
  daily: Map<string, UsageDailyRow>
  firstDate: string | null
  lastDate: string | null
  options: ResolvedUsageStatsOptions
}

const DEFAULT_DAY_COUNT = 30
const AUTO_MODEL = "auto"

function emptyBreakdown(): UsageBreakdown {
  return { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }
}

function addUsage(target: UsageBreakdown, usage: TurnUsage): void {
  target.input += usage.input_tokens
  target.output += usage.output_tokens
  target.cacheRead += usage.cache_read_input_tokens
  target.cacheWrite += usage.cache_creation_input_tokens
}

export function usageTotal(usage: UsageBreakdown): number {
  return usage.input + usage.output + usage.cacheRead + usage.cacheWrite
}

export function cacheHitRate(usage: UsageBreakdown): number {
  const promptTokens = usage.input + usage.cacheRead + usage.cacheWrite
  return promptTokens > 0 ? usage.cacheRead / promptTokens : 0
}

function dateKey(value: string): string | null {
  const key = value.slice(0, 10)
  return /^\d{4}-\d{2}-\d{2}$/.test(key) ? key : null
}

function addDays(date: Date, days: number): Date {
  const next = new Date(date)
  next.setUTCDate(next.getUTCDate() + days)
  return next
}

function toDateKey(date: Date): string {
  return date.toISOString().slice(0, 10)
}

function parseDateKey(key: string): Date {
  return new Date(`${key}T00:00:00.000Z`)
}

function inclusiveDaySpan(firstDate: string, lastDate: string): number {
  const start = parseDateKey(firstDate).getTime()
  const end = parseDateKey(lastDate).getTime()
  return Math.max(1, Math.round((end - start) / 86_400_000) + 1)
}

function modelName(detail: ConversationDetail): string {
  const summaryModel = detail.summary.model?.trim()
  if (summaryModel) return summaryModel

  for (let i = detail.turns.length - 1; i >= 0; i -= 1) {
    const turnModel = detail.turns[i]?.model?.trim()
    if (turnModel) return turnModel
  }

  return AUTO_MODEL
}

function buildDailyRows({
  daily,
  firstDate,
  lastDate,
  options,
}: DailyRowsInput): UsageDailyRow[] {
  const anchor = lastDate ? parseDateKey(lastDate) : options.now
  const start = addDays(anchor, -(options.dayCount - 1))

  return Array.from({ length: options.dayCount }, (_, index) => {
    const key = toDateKey(addDays(start, index))
    const row = daily.get(key)
    if (row) return { ...row }

    const empty = emptyBreakdown()
    return {
      date: key,
      sessions: 0,
      total: 0,
      cacheHitRate: cacheHitRate(empty),
      ...empty,
    }
  }).filter((row) => firstDate === null || row.date >= firstDate)
}

function createAccumulator(): UsageAccumulator {
  return {
    total: emptyBreakdown(),
    models: new Map(),
    daily: new Map(),
    sessionCount: 0,
    firstDate: null,
    lastDate: null,
  }
}

function addModelUsage(
  accumulator: UsageAccumulator,
  detail: ConversationDetail,
  usage: TurnUsage
): void {
  const model = modelName(detail)
  const row = accumulator.models.get(model) ?? {
    model,
    sessions: 0,
    total: 0,
    ...emptyBreakdown(),
  }
  row.sessions += 1
  addUsage(row, usage)
  row.total = usageTotal(row)
  accumulator.models.set(model, row)
}

function addDailyUsage(
  accumulator: UsageAccumulator,
  detail: ConversationDetail,
  usage: TurnUsage
): void {
  const day = dateKey(detail.summary.started_at)
  if (!day) return

  if (accumulator.firstDate === null || day < accumulator.firstDate) {
    accumulator.firstDate = day
  }
  if (accumulator.lastDate === null || day > accumulator.lastDate) {
    accumulator.lastDate = day
  }

  const row = accumulator.daily.get(day) ?? {
    date: day,
    sessions: 0,
    total: 0,
    cacheHitRate: 0,
    ...emptyBreakdown(),
  }
  row.sessions += 1
  addUsage(row, usage)
  row.total = usageTotal(row)
  row.cacheHitRate = cacheHitRate(row)
  accumulator.daily.set(day, row)
}

function addDetailUsage(
  accumulator: UsageAccumulator,
  detail: ConversationDetail
): void {
  const usage = detail.session_stats?.total_usage
  if (!usage) return

  accumulator.sessionCount += 1
  addUsage(accumulator.total, usage)
  addModelUsage(accumulator, detail, usage)
  addDailyUsage(accumulator, detail, usage)
}

export function aggregateUsageStats(
  details: ConversationDetail[],
  options: UsageStatsOptions = {}
): UsageDashboardStats {
  const resolvedOptions = {
    dayCount: options.dayCount ?? DEFAULT_DAY_COUNT,
    now: options.now ?? new Date(),
  }

  const accumulator = createAccumulator()
  details.forEach((detail) => addDetailUsage(accumulator, detail))
  const { daily, firstDate, lastDate, models, sessionCount, total } =
    accumulator
  const daySpan =
    firstDate && lastDate ? inclusiveDaySpan(firstDate, lastDate) : 1

  return {
    total,
    totalTokens: usageTotal(total),
    sessionCount,
    cacheHitRate: cacheHitRate(total),
    averageDailySessions: sessionCount / daySpan,
    firstDate,
    lastDate,
    modelRows: Array.from(models.values()).sort((a, b) => b.total - a.total),
    dailyRows: buildDailyRows({
      daily,
      firstDate,
      lastDate,
      options: resolvedOptions,
    }),
  }
}
