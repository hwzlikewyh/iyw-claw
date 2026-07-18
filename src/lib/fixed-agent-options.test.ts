import { describe, expect, it } from "vitest"

import { ALL_AGENT_TYPES } from "@/lib/types"
import { getFixedAgentOptions } from "@/lib/fixed-agent-options"

describe("fixed agent options", () => {
  it("provides a synchronous snapshot for every agent", () => {
    for (const agentType of ALL_AGENT_TYPES) {
      const snapshot = getFixedAgentOptions(agentType)

      expect(snapshot).toEqual(
        expect.objectContaining({
          config_options: expect.any(Array),
          available_commands: [],
        })
      )
    }
  })

  it("pins the supported Codex approval presets", () => {
    const snapshot = getFixedAgentOptions("codex")
    const mode = snapshot.config_options.find((option) => option.id === "mode")

    expect(mode?.kind.type).toBe("select")
    if (mode?.kind.type !== "select") return
    expect(mode.kind.current_value).toBe("agent")
    expect(mode.kind.options.map((option) => option.value)).toEqual([
      "read-only",
      "agent",
      "agent-full-access",
    ])
  })

  it("pins the managed Grok model and reasoning choices", () => {
    const snapshot = getFixedAgentOptions("grok")
    const model = snapshot.config_options.find(
      (option) => option.id === "model"
    )
    const effort = snapshot.config_options.find(
      (option) => option.id === "reasoning_effort"
    )

    expect(model?.kind.type).toBe("select")
    expect(effort?.kind.type).toBe("select")
    if (model?.kind.type !== "select" || effort?.kind.type !== "select") {
      return
    }
    expect(model.kind.options.map((option) => option.value)).toEqual([
      "deepseek-v4-pro",
      "doubao-seed-2-1-pro-260628",
      "deepseek-v4-flash",
    ])
    expect(effort.kind.options.map((option) => option.value)).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
    ])
  })

  it("localizes chat options when a translator is provided", () => {
    const messages: Record<string, string> = {
      "sessionConfig.options.mode.name": "权限模式",
      "sessionConfig.values.mode.readOnly.name": "只读",
      "sessionConfig.values.mode.agent.name": "智能体",
      "sessionConfig.values.mode.agentFullAccess.name": "智能体（完全访问）",
    }
    const snapshot = getFixedAgentOptions(
      "codex",
      {},
      (key) => messages[key] ?? key
    )
    const mode = snapshot.config_options[0]

    expect(mode.name).toBe("权限模式")
    if (mode.kind.type !== "select") return
    expect(mode.kind.options.map((option) => option.name)).toEqual([
      "只读",
      "智能体",
      "智能体（完全访问）",
    ])
  })
})
