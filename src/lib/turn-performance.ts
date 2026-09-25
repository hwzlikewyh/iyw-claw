const MAX_RECORDED_TURNS = 1_000
const turnTimings = new Map<string, TurnTimings>()

export interface TurnTimings {
  firstActivityMs: number | null
  firstThinkingMs: number | null
  firstTextMs: number | null
}

interface LiveTiming {
  startedAt: number
  firstActivityAt?: number | null
  firstThinkingAt?: number | null
  firstTextAt?: number | null
  content: { type: string; text?: string }[]
}

// 恢复历史块时无法推断首事件时间；只记录本机实际观察到的首次事件。
export function observeTurnTiming<T extends LiveTiming>(
  message: T,
  kind: "text" | "thinking" | "tool_call" | "plan"
): T {
  const now = Date.now()
  const firstActivityAt =
    message.firstActivityAt ?? (message.content.length === 0 ? now : null)
  const firstThinkingAt =
    message.firstThinkingAt ??
    (kind === "thinking" &&
    !message.content.some((block) => block.type === "thinking")
      ? now
      : null)
  const firstTextAt =
    message.firstTextAt ??
    (kind === "text" &&
    !message.content.some(
      (block) => block.type === "text" && Boolean(block.text)
    )
      ? now
      : null)
  if (
    firstActivityAt === message.firstActivityAt &&
    firstThinkingAt === message.firstThinkingAt &&
    firstTextAt === message.firstTextAt
  )
    return message
  return { ...message, firstActivityAt, firstThinkingAt, firstTextAt }
}

export function measuredTurnTimings(
  timing: Omit<LiveTiming, "content">
): TurnTimings {
  const elapsed = (at: number | null | undefined) => {
    if (at == null) return null
    const value = at - timing.startedAt
    return Number.isFinite(value) && value >= 0 ? value : null
  }
  return {
    firstActivityMs: elapsed(timing.firstActivityAt),
    firstThinkingMs: elapsed(timing.firstThinkingAt),
    firstTextMs: elapsed(timing.firstTextAt),
  }
}

export function rememberTurnTimings(messageId: string, timing: TurnTimings) {
  turnTimings.set(messageId, timing)
  if (turnTimings.size > MAX_RECORDED_TURNS) {
    turnTimings.delete(turnTimings.keys().next().value!)
  }
}

export function recordedTurnTimings(messageId: string): TurnTimings | null {
  return turnTimings.get(messageId) ?? null
}

export function firstTokenElapsed(timing: {
  startedAt: number
  firstTextAt?: number | null
}): number | null {
  if (timing.firstTextAt == null) return null
  const elapsed = timing.firstTextAt - timing.startedAt
  return Number.isFinite(elapsed) && elapsed >= 0 ? elapsed : null
}

export function rememberFirstTokenTime(
  messageId: string,
  milliseconds: number | null
) {
  if (milliseconds == null) return
  rememberTurnTimings(messageId, {
    firstActivityMs: turnTimings.get(messageId)?.firstActivityMs ?? null,
    firstThinkingMs: turnTimings.get(messageId)?.firstThinkingMs ?? null,
    firstTextMs: milliseconds,
  })
}

export function recordedFirstTokenTime(messageId: string): number | null {
  return turnTimings.get(messageId)?.firstTextMs ?? null
}
