import { describe, expect, it } from "vitest"

import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import {
  countProcessItems,
  processPartHasError,
  splitAssistantTurnParts,
} from "./assistant-turn-model"

describe("splitAssistantTurnParts", () => {
  it("keeps every reasoning block inside one ordered process stream", () => {
    const parts: AdaptedContentPart[] = [
      { type: "reasoning", content: "first thought", isStreaming: false },
      { type: "text", text: "I will inspect the source." },
      { type: "reasoning", content: "second thought", isStreaming: false },
      { type: "text", text: "Final answer" },
    ]

    const sections = splitAssistantTurnParts(parts, true)

    expect(sections.processParts).toEqual(parts.slice(0, 3))
    expect(sections.summaryParts).toEqual([parts[3]])
    expect(sections.resultParts).toEqual([])
  })

  it("keeps generated results outside the process surface", () => {
    const result = {
      type: "displayed-image" as const,
      caption: "result",
      image: { data: "abc", mime_type: "image/png", name: "result.png" },
      sourceKind: null,
    }
    const parts: AdaptedContentPart[] = [
      { type: "reasoning", content: "thinking", isStreaming: true },
      result,
    ]

    const sections = splitAssistantTurnParts(parts, false)

    expect(sections.processParts).toEqual([parts[0]])
    expect(sections.resultParts).toEqual([result])
  })
})

describe("process summaries", () => {
  it("counts grouped tools and preserves their error state", () => {
    const group: AdaptedContentPart = {
      type: "tool-group",
      isStreaming: false,
      items: [
        {
          type: "tool-call",
          toolCallId: "one",
          toolName: "Read",
          input: null,
          output: null,
          state: "output-available",
        },
        {
          type: "tool-call",
          toolCallId: "two",
          toolName: "Read",
          input: null,
          output: null,
          errorText: "failed",
          state: "output-error",
        },
      ],
    }

    expect(countProcessItems([group])).toBe(2)
    expect(processPartHasError(group)).toBe(true)
  })
})
