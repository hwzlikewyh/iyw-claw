import { describe, expect, it } from "vitest"

import { AGENT_DISPLAY_ORDER, ALL_AGENT_TYPES } from "./types"

describe("agent display order", () => {
  it("prioritizes Codex, Hermes, OpenCode, OpenClaw and CodeBuddy", () => {
    expect(AGENT_DISPLAY_ORDER.slice(0, 5)).toEqual([
      "codex",
      "hermes",
      "open_code",
      "open_claw",
      "code_buddy",
    ])
    expect(ALL_AGENT_TYPES).toEqual(AGENT_DISPLAY_ORDER)
  })
})
