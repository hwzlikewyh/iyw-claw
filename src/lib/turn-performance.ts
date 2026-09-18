const MAX_RECORDED_TURNS = 1_000
const firstTokenTimes = new Map<string, number>()

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
  firstTokenTimes.set(messageId, milliseconds)
  if (firstTokenTimes.size > MAX_RECORDED_TURNS) {
    firstTokenTimes.delete(firstTokenTimes.keys().next().value!)
  }
}

export function recordedFirstTokenTime(messageId: string): number | null {
  return firstTokenTimes.get(messageId) ?? null
}
