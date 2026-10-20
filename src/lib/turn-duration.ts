import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { MessageTurn, SessionActivitySnapshot } from "@/lib/types"

export function liveTurnStartedAt(
  activity?: SessionActivitySnapshot | null,
  now = Date.now()
): number {
  const startedAt = Date.parse(activity?.started_at ?? "")
  const sampledAt = Date.parse(activity?.sampled_at ?? "")
  const elapsed = sampledAt - startedAt
  return Number.isFinite(elapsed) && elapsed >= 0 ? now - elapsed : now
}

export function completeLiveTurnTiming(
  message: LiveMessage,
  activity?: SessionActivitySnapshot | null
): LiveMessage {
  // 完成时间只结算一次，重放或重复通知不能延长已结束的轮次。
  if (message.completedAt != null) return message
  const startedAt = Date.parse(activity?.started_at ?? "")
  const completedAt = Date.parse(activity?.sampled_at ?? "")
  if (
    !Number.isFinite(startedAt) ||
    !Number.isFinite(completedAt) ||
    completedAt < startedAt
  ) {
    return { ...message, completedAt: message.completedAt ?? Date.now() }
  }
  // 完成事件的两个时间都来自后端，避免事件重放或客户端时钟偏差放大耗时。
  // 首次活动仍保留客户端已测得的相对时长，只对齐绝对时间基准。
  const offset = startedAt - message.startedAt
  const align = (at: number | null | undefined) =>
    at == null ? at : at + offset
  return {
    ...message,
    startedAt,
    completedAt,
    firstActivityAt: align(message.firstActivityAt),
    firstThinkingAt: align(message.firstThinkingAt),
    firstTextAt: align(message.firstTextAt),
  }
}

export function resolveTurnDuration(input: {
  duration_ms?: number | null
  startedAt?: string | null
  completedAt?: string | null
}): number | null {
  const duration = input.duration_ms
  if (
    typeof duration === "number" &&
    Number.isFinite(duration) &&
    duration >= 0
  )
    return duration
  if (!input.startedAt || !input.completedAt) return null
  const elapsed = Date.parse(input.completedAt) - Date.parse(input.startedAt)
  return Number.isFinite(elapsed) && elapsed >= 0 ? elapsed : null
}

export function sumTurnDurations(
  turns: { duration_ms?: number | null; completed_at?: string | null }[]
): number | null {
  const intervals: [number, number][] = []
  let total: number | null = null
  for (const turn of turns) {
    const duration = resolveTurnDuration({ duration_ms: turn.duration_ms })
    if (duration === null) continue
    total ??= 0
    const end = Date.parse(turn.completed_at ?? "")
    if (Number.isFinite(end)) intervals.push([end - duration, end])
    else total += duration
  }
  // 多条消息可能携带同一轮的累计耗时，重叠区间只能计算一次。
  let previousEnd = Number.NEGATIVE_INFINITY
  for (const [start, end] of intervals.sort(
    (left, right) => left[0] - right[0]
  )) {
    total = (total ?? 0) + Math.max(0, end - Math.max(start, previousEnd))
    previousEnd = Math.max(previousEnd, end)
  }
  return total
}

export function completeTurnTiming(
  turns: MessageTurn[],
  timing: { startedAt: number; completedAt: number }
): MessageTurn[] {
  let startedAt = timing.startedAt
  const completedAt = new Date(timing.completedAt).toISOString()
  return turns.map((turn, index) => {
    if (turn.role === "user") {
      const timestamp = Date.parse(turn.timestamp)
      if (Number.isFinite(timestamp)) startedAt = timestamp
      return turn
    }
    if (turn.role !== "assistant") return turn
    const next = turns[index + 1]
    if (next?.role === "assistant") return turn
    // 中途追加输入会拆分展示轮次，按输入边界结算，避免合并时重复累计。
    const end = next?.role === "user" ? next.timestamp : completedAt
    return {
      ...turn,
      duration_ms: resolveTurnDuration({
        duration_ms: turn.duration_ms,
        startedAt: new Date(startedAt).toISOString(),
        completedAt: end,
      }),
      completed_at: turn.completed_at ?? end,
    }
  })
}

export function completeHistoryTurnTiming(
  segments: MessageTurn[],
  source: MessageTurn
): MessageTurn[] {
  const duration = resolveTurnDuration({ duration_ms: source.duration_ms })
  const completedAt = Date.parse(source.completed_at ?? "")
  if (duration === null || !Number.isFinite(completedAt)) return segments
  return completeTurnTiming(segments, {
    startedAt: completedAt - duration,
    completedAt,
  })
}
