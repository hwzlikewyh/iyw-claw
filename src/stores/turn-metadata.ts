import type { MessageTurn, TurnUsage } from "@/lib/types"

export interface TurnMetadataPatch {
  index: number
  usage?: TurnUsage | null
  duration_ms?: number | null
  model?: string | null
  completed_at?: string | null
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
}): TurnMetadataPatch[] {
  const historyBoundary = Math.min(
    Math.max(params.persistedAssistantCount, 0),
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
