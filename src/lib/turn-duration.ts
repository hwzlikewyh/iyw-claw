import type { MessageTurn } from "@/lib/types"

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
