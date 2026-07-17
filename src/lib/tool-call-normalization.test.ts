import { describe, expect, it } from "vitest"

import {
  inferLiveToolName,
  normalizeToolName,
} from "@/lib/tool-call-normalization"

describe("Grok tool-call normalization", () => {
  it("renders historical terminal calls through the terminal card", () => {
    expect(normalizeToolName("run_terminal_command")).toBe("bash")
  })

  it("prefers an unwrapped companion title over a generic input shape", () => {
    expect(
      inferLiveToolName({
        title: "iyw-claw-mcp__cancel_delegation",
        kind: "other",
        rawInput: JSON.stringify({ task_id: "task-1" }),
        meta: { "x.ai/tool": { name: "use_tool" } },
      })
    ).toBe("cancel_delegation")
  })
})
