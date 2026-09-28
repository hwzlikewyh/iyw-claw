import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { MessageTurn, SessionActivitySnapshot } from "@/lib/types"

export function completeLiveTurnTiming(
  message: LiveMessage,
  activity?: SessionActivitySnapshot | null
): LiveMessage {
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
