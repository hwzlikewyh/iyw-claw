import type { EventEnvelope } from "@/lib/types"
import { isContextCompactionMeta } from "@/lib/context-compaction"

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
  content: {
    type: string
    text?: string
    entries?: readonly unknown[]
    info?: { meta?: unknown }
  }[]
}

function parseTimestamp(value?: string | null): number {
  return Date.parse(value ?? "")
}

function eventTimestamp(event: EventEnvelope): number | null {
  const activity = event.activity
  if (!activity) return null
  switch (event.type) {
    case "content_delta":
      return event.text.trim() ? parseTimestamp(activity.text_at) : null
    case "thinking":
      return event.text.trim() ? parseTimestamp(activity.thinking_at) : null
    case "tool_call":
      return isContextCompactionMeta(event.meta)
        ? null
        : parseTimestamp(activity.tool_started_at)
    case "plan_update":
      return event.entries.length > 0 ? Date.parse(activity.sampled_at) : null
    default:
      return null
  }
}

function hasModelResponse(block: LiveTiming["content"][number]): boolean {
  if (block.type === "user_input") return false
  if (block.type === "text" || block.type === "thinking")
    return Boolean(block.text?.trim())
  if (block.type === "plan") return Boolean(block.entries?.length)
  if (block.type === "tool_call")
    return !isContextCompactionMeta(block.info?.meta)
  return true
}

function firstContentAt(
  message: LiveTiming,
  kind: "text" | "thinking",
  observation: { event: EventEnvelope; at: number }
): number | null {
  const previous =
    kind === "thinking" ? message.firstThinkingAt : message.firstTextAt
  if (previous != null) return previous
  const { event } = observation
  const eventKind = kind === "text" ? "content_delta" : "thinking"
  if (
    (event.type !== "content_delta" && event.type !== "thinking") ||
    event.type !== eventKind ||
    !event.text.trim()
  )
    return null
  const seen = message.content.some(
    (block) => block.type === kind && Boolean(block.text?.trim())
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
  const observation = { event, at: observedAt }
  const timings = {
    firstActivityAt:
      message.firstActivityAt ??
      (message.content.some(hasModelResponse) ? null : observedAt),
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

// 首字按本轮首次有效返回计时：工具调用、计划、思考或正文，忽略空事件。
export function firstTokenDuration(timing: TurnTimings | null): number | null {
  const values = [
    timing?.firstActivityMs,
    timing?.firstThinkingMs,
    timing?.firstTextMs,
  ].filter(
    (value): value is number =>
      value != null && Number.isFinite(value) && value >= 0
  )
  return values.length > 0 ? Math.min(...values) : null
}

export function firstTokenElapsed(
  timing: Omit<LiveTiming, "content">
): number | null {
  return firstTokenDuration(measuredTurnTimings(timing))
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
  return firstTokenDuration(recordedTurnTimings(messageId))
}
