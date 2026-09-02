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

    expect(sections.processParts).toEqual([parts[0], parts[1], parts[2]])
    expect(sections.reasoningParts).toEqual([parts[0], parts[2]])
    expect(sections.responseParts).toEqual([parts[3]])
    expect(sections.resultParts).toEqual([])
  })

  it("keeps live body text in the ordered process stream", () => {
    const parts: AdaptedContentPart[] = [
      { type: "text", text: "先说明执行范围。" },
      {
        type: "tool-call",
        toolCallId: "read",
        toolName: "Read",
        input: null,
        output: null,
        state: "input-available",
      },
      { type: "text", text: "正在整理结果。" },
    ]

    const sections = splitAssistantTurnParts(parts, false)

    expect(sections.processParts).toEqual(parts)
    expect(sections.responseParts).toEqual([])
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
    expect(sections.reasoningParts).toEqual([parts[0]])
  })

  it("does not create empty process rows for whitespace text", () => {
    const sections = splitAssistantTurnParts(
      [
        { type: "text", text: "   " },
        { type: "text", text: "过程说明" },
        { type: "text", text: "最终答复" },
      ],
      true
    )

    expect(sections.processParts).toEqual([{ type: "text", text: "过程说明" }])
    expect(sections.responseParts).toEqual([{ type: "text", text: "最终答复" }])
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
