import type { EventEnvelope } from "@/lib/types"

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

function parseTimestamp(value?: string | null): number {
  return Date.parse(value ?? "")
}

function eventTimestamp(event: EventEnvelope): number | null {
  const activity = event.activity
  if (!activity) return null
  switch (event.type) {
    case "content_delta":
      return event.text.length > 0 ? parseTimestamp(activity.text_at) : null
    case "thinking":
      return parseTimestamp(
        event.text.length > 0 ? activity.thinking_at : activity.sampled_at
      )
    case "tool_call":
      return parseTimestamp(activity.tool_started_at)
    case "plan_update":
      return event.entries.length > 0 ? Date.parse(activity.sampled_at) : null
    default:
      return null
  }
}

function firstContentAt(
  message: LiveTiming,
  kind: "text" | "thinking",
  observation: { type: string; at: number }
): number | null {
  const previous =
    kind === "thinking" ? message.firstThinkingAt : message.firstTextAt
  if (previous != null) return previous
  const eventKind = kind === "text" ? "content_delta" : "thinking"
  if (observation.type !== eventKind) return null
  const seen = message.content.some(
    (block) =>
      block.type === kind && (kind === "thinking" || Boolean(block.text))
  )
  return seen ? null : observation.at
}

// 在流式批处理之前记录后端事件时间；恢复内容不能补造首次发生时间。
export function observeTurnTiming<T extends LiveTiming>(
  message: T,
  event: EventEnvelope
): T {
  const at = eventTimestamp(event)
  const startedAt = parseTimestamp(event.activity?.started_at)
  if (at == null || !Number.isFinite(at) || !Number.isFinite(startedAt))
    return message
  const elapsed = at - startedAt
  if (elapsed < 0) return message
  const observedAt = message.startedAt + elapsed
  const observation = { type: event.type, at: observedAt }
  const timings = {
    firstActivityAt:
      message.firstActivityAt ??
      (message.content.every((block) => block.type === "user_input")
        ? observedAt
        : null),
    firstThinkingAt: firstContentAt(message, "thinking", observation),
    firstTextAt: firstContentAt(message, "text", observation),
  }
  const keys = ["firstActivityAt", "firstThinkingAt", "firstTextAt"] as const
  return keys.every((key) => timings[key] === message[key])
    ? message
    : { ...message, ...timings }
}

export function alignTurnTimings<T extends LiveTiming>(
  message: T,
  previous: LiveTiming
): T {
  const offset = message.startedAt - previous.startedAt
  const align = (at: number | null | undefined) =>
    at == null ? at : at + offset
  return {
    ...message,
    firstActivityAt: align(previous.firstActivityAt),
    firstThinkingAt: align(previous.firstThinkingAt),
    firstTextAt: align(previous.firstTextAt),
  }
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
