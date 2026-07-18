import { describe, expect, it } from "vitest"

import { effectiveSelections } from "@/components/automations/agent-config-section"
import type { AgentOptionsSnapshot } from "@/lib/types"

describe("automation agent config", () => {
  it("keeps the displayed mode when model options are also available", () => {
    const snapshot: AgentOptionsSnapshot = {
      modes: {
        current_mode_id: "agent",
        available_modes: [{ id: "agent", name: "Agent" }],
      },
      config_options: [
        {
          id: "model",
          name: "Model",
          category: "model",
          kind: {
            type: "select",
            current_value: "online-model",
            options: [{ value: "online-model", name: "Online Model" }],
            groups: [],
          },
        },
      ],
      available_commands: [],
    }

    expect(effectiveSelections(snapshot, null, {})).toEqual({
      mode_id: "agent",
      config_values: { model: "online-model" },
    })
  })
})
