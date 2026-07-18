import { describe, expect, it } from "vitest"

import {
  buildAgentOptionsSnapshot,
  getLocalAgentModelIds,
  mergeAgentModels,
  parseGatewayModels,
} from "@/lib/gateway-model-catalog"

describe("gateway model catalog", () => {
  it("parses model display metadata and reasoning defaults", () => {
    const models = parseGatewayModels({
      data: [
        {
          id: "gpt-5.4",
          display_name: "GPT-5.4",
          description: "General reasoning",
          reasoning: {
            efforts: ["minimal", "high"],
            default_effort: "high",
          },
        },
      ],
    })

    expect(models).toEqual([
      {
        id: "gpt-5.4",
        name: "GPT-5.4",
        description: "General reasoning",
        efforts: ["minimal", "high"],
        defaultEffort: "high",
      },
    ])
  })

  it("rejects malformed or empty payloads", () => {
    expect(parseGatewayModels(null)).toEqual([])
    expect(parseGatewayModels({ data: [{ id: "" }] })).toEqual([])
    expect(parseGatewayModels({ data: [] })).toEqual([])
  })

  it("keeps local models when the gateway does not return them", () => {
    const localIds = getLocalAgentModelIds("grok")
    const models = mergeAgentModels("grok", [
      {
        id: localIds[0],
        name: "Remote name",
        description: "Remote description",
        efforts: ["low", "high"],
        defaultEffort: "high",
      },
    ])

    expect(models.map((model) => model.id)).toEqual(localIds)
    expect(models[0]).toMatchObject({
      name: "Remote name",
      efforts: ["low", "high"],
      defaultEffort: "high",
    })
  })

  it("keeps mode and model catalogs specific to each agent", () => {
    const codex = buildAgentOptionsSnapshot("codex")
    const claude = buildAgentOptionsSnapshot("claude_code")
    const codexModel = codex.config_options.find((item) => item.id === "model")
    const claudeModel = claude.config_options.find(
      (item) => item.id === "model"
    )

    expect(codex.modes?.available_modes.map((mode) => mode.id)).toEqual([
      "read-only",
      "plan",
      "agent",
      "agent-full-access",
    ])
    expect(claude.modes?.available_modes.map((mode) => mode.id)).toEqual([
      "default",
      "acceptEdits",
      "plan",
      "bypassPermissions",
    ])
    expect(codexModel?.kind.options.map((item) => item.value)).toEqual([
      "gpt-5.4",
      "deepseek-v4-pro",
      "deepseek-v4-flash",
    ])
    expect(claudeModel?.kind.options.map((item) => item.value)).toEqual([
      "claude-opus-4-6",
      "gpt-5.4",
    ])
  })
})
