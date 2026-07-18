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

  it("provides Codex modes immediately without ACP", () => {
    const snapshot = getFixedAgentOptions("codex")
    const mode = snapshot.modes

    expect(mode?.current_mode_id).toBe("read-only")
    expect(mode?.available_modes.map((option) => option.id)).toEqual([
      "read-only",
      "plan",
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
      "gpt-5.4",
      "claude-opus-4-6",
      "deepseek-v4-pro",
      "deepseek-v4-flash",
      "doubao-seed-2-1-pro-260628",
      "gemini-3.1-pro-preview",
      "qwen3.7-max",
    ])
    expect(effort.kind.options.map((option) => option.value)).toEqual([
      "minimal",
      "low",
      "medium",
      "high",
      "xhigh",
    ])
  })

  it("localizes chat options when a translator is provided", () => {
    const messages: Record<string, string> = {
      "sessionConfig.options.model.name": "模型",
    }
    const snapshot = getFixedAgentOptions(
      "codex",
      {},
      (key) => messages[key] ?? key
    )
    const mode = snapshot.config_options.find((option) => option.id === "model")

    expect(mode?.name).toBe("模型")
    if (!mode || mode.kind.type !== "select") return
    expect(mode.kind.options.map((option) => option.name)).toEqual([
      "GPT-5.4",
      "DeepSeek V4 Pro",
      "DeepSeek V4 Flash",
    ])
  })
})
