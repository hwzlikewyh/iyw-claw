import { describe, expect, it } from "vitest"
import { computeTurnMetadataPatches } from "@/stores/conversation-runtime-store"
import type { MessageTurn, TurnUsage } from "@/lib/types"

function usage(input: number): TurnUsage {
  return {
    input_tokens: input,
    output_tokens: 0,
    cache_creation_input_tokens: 0,
    cache_read_input_tokens: 0,
  }
}

function assistant(
  id: string,
  durationMs: number,
  inputTokens?: number
): MessageTurn {
  return {
    id,
    role: "assistant",
    blocks: [],
    timestamp: "2026-01-01T00:00:00Z",
    duration_ms: durationMs,
    usage: inputTokens === undefined ? undefined : usage(inputTokens),
  }
}

describe("computeTurnMetadataPatches", () => {
  it("excludes persisted history from the first resumed reply", () => {
    const patches = computeTurnMetadataPatches({
      localAssistantIndices: [1],
      parsedAssistantTurns: [
        assistant("h0", 5000, 100),
        assistant("h1", 7000, 200),
        assistant("new", 1234, 50),
      ],
      persistedAssistantCount: 2,
    })

    expect(patches).toEqual([
      {
        index: 1,
        duration_ms: 1234,
        usage: usage(50),
        model: undefined,
        completed_at: undefined,
      },
    ])
  })

  it("folds only current-session parser sub-turns", () => {
    const patches = computeTurnMetadataPatches({
      localAssistantIndices: [1],
      parsedAssistantTurns: [
        assistant("history", 9000, 300),
        assistant("part-1", 400, 4),
        assistant("part-2", 600, 6),
      ],
      persistedAssistantCount: 1,
    })

    expect(patches[0]?.duration_ms).toBe(1000)
    expect(patches[0]?.usage).toEqual(usage(10))
  })

  it("head-aligns a lagging parse instead of shifting stats forward", () => {
    const patches = computeTurnMetadataPatches({
      localAssistantIndices: [1, 3],
      parsedAssistantTurns: [
        assistant("history", 5000),
        assistant("first-new", 111, 11),
      ],
      persistedAssistantCount: 1,
    })

    expect(patches).toHaveLength(1)
    expect(patches[0]).toMatchObject({
      index: 1,
      duration_ms: 111,
      usage: usage(11),
    })
  })
})
