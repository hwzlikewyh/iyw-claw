import type { MessageTurn, TurnUsage } from "@/lib/types"

export interface TurnMetadataPatch {
  index: number
  fork_message_id?: string | null
  usage?: TurnUsage | null
  duration_ms?: number | null
  model?: string | null
  completed_at?: string | null
}

export function resolveForkMessageId(
  local: MessageTurn,
  parsed: MessageTurn[]
) {
  let text = ""
  for (const block of local.blocks) {
    if (block.type === "tool_use" || block.type === "tool_result") text = ""
    else if (block.type === "text") text += block.text
  }
  text = text.trim()
  if (!text) return undefined
  const startedAt = Date.parse(local.timestamp)
  const completedAt = Date.parse(local.completed_at ?? "")
  const matches = parsed.filter((turn) => {
    if (!turn.fork_message_id) return false
    const nativeCompletedAt = Date.parse(turn.completed_at ?? "")
    if (!(nativeCompletedAt >= startedAt && nativeCompletedAt <= completedAt))
      return false
    const ending = turn.blocks
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join("")
      .trim()
    return ending === text
  })
  // 用正文确认唯一终点，不能把耗时统计的按数量对齐直接当成消息身份。
  return matches.length === 1 ? matches[0].fork_message_id : undefined
}

function mergeUsage(current: TurnUsage | null | undefined, extra: TurnUsage) {
  if (!current) return { ...extra }
  return {
    input_tokens: current.input_tokens + extra.input_tokens,
    output_tokens: current.output_tokens + extra.output_tokens,
    cache_creation_input_tokens:
      current.cache_creation_input_tokens + extra.cache_creation_input_tokens,
    cache_read_input_tokens:
      current.cache_read_input_tokens + extra.cache_read_input_tokens,
  }
}

export function computeTurnMetadataPatches(params: {
  localAssistantIndices: number[]
  parsedAssistantTurns: MessageTurn[]
  persistedAssistantCount: number
  parsedAssistantTurnsBefore?: number
}): TurnMetadataPatch[] {
  const historyBoundary = Math.min(
    Math.max(
      params.persistedAssistantCount - (params.parsedAssistantTurnsBefore ?? 0),
      0
    ),
    params.parsedAssistantTurns.length
  )
  const sessionTurns = params.parsedAssistantTurns.slice(historyBoundary)
  const offset = sessionTurns.length - params.localAssistantIndices.length
  const patches: TurnMetadataPatch[] = []

  for (let i = 0; i < params.localAssistantIndices.length; i++) {
    const parsedIndex = Math.max(offset, 0) + i
    const parsed = sessionTurns[parsedIndex]
    let usage = parsed?.usage
    let durationMs = parsed?.duration_ms
    let model = parsed?.model
    const completedAt = parsed?.completed_at

    if (i === 0 && offset > 0) {
      for (let j = 0; j < offset; j++) {
        const extra = sessionTurns[j]
        if (extra.usage) usage = mergeUsage(usage, extra.usage)
        if (typeof extra.duration_ms === "number") {
          durationMs = (durationMs ?? 0) + extra.duration_ms
        }
        if (!model && extra.model) model = extra.model
      }
    }

    if (!usage && !durationMs && !model && !completedAt) continue
    patches.push({
      index: params.localAssistantIndices[i],
      usage,
      duration_ms: durationMs,
      model,
      completed_at: completedAt,
    })
  }

  return patches
}
