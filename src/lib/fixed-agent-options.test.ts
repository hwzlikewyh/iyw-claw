import { describe, expect, it } from "vitest"

import { getFixedAgentOptions } from "@/lib/fixed-agent-options"
import { ALL_AGENT_TYPES } from "@/lib/types"

describe("fixed agent options", () => {
  it("provides modes synchronously for every agent", () => {
    for (const agentType of ALL_AGENT_TYPES) {
      const snapshot = getFixedAgentOptions(agentType)

      expect(snapshot.modes?.available_modes.length).toBeGreaterThan(0)
      expect(snapshot.available_commands).toEqual([])
    }
  })

  it("does not invent local model choices before online data is available", () => {
    for (const agentType of ALL_AGENT_TYPES) {
      const snapshot = getFixedAgentOptions(agentType)

      expect(snapshot.config_options).toEqual([])
    }
  })

  it("exposes Codex mode presets through the mode selector", () => {
    const snapshot = getFixedAgentOptions("codex")

    expect(snapshot.modes?.available_modes.map((mode) => mode.id)).toEqual([
      "read-only",
      "plan",
      "agent",
      "agent-full-access",
    ])
  })
})
