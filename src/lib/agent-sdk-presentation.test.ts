import { describe, expect, it } from "vitest"

import {
  AGENT_SDK_ALIASES,
  maskAgentSdkBrandText,
  presentAgentSdkAgents,
} from "@/lib/agent-sdk-presentation"
import { ALL_AGENT_TYPES, type AcpAgentInfo, type AgentType } from "@/lib/types"

function makeAgent(agentType: AgentType, name: string): AcpAgentInfo {
  return {
    agent_type: agentType,
    registry_id: agentType,
    registry_version: null,
    name,
    description: `${name} description`,
    available: true,
    distribution_type: "binary",
    enabled: true,
    sort_order: ALL_AGENT_TYPES.indexOf(agentType),
    installed_version: null,
    env: {},
    config_json: null,
    config_file_path: null,
    opencode_auth_json: null,
    codex_auth_json: null,
    codex_config_toml: null,
    cline_secrets_json: null,
    hermes_config_yaml: null,
    model_provider_id: null,
  }
}

describe("Agent SDK presentation", () => {
  const agents = ALL_AGENT_TYPES.map((type) =>
    makeAgent(
      type,
      type === "hermes"
        ? "Hermes Agent"
        : type === "open_claw"
          ? "OpenClaw"
          : type
    )
  )

  it("exposes every registered Agent SDK platform", () => {
    const presented = presentAgentSdkAgents(agents, (name) => `${name} SDK`)

    expect(presented.map((agent) => agent.agent_type)).toEqual(ALL_AGENT_TYPES)
  })

  it("uses neutral aliases except for Hermes Agent and OpenClaw", () => {
    expect(AGENT_SDK_ALIASES).toEqual({
      codex: "星河",
      open_code: "云舟",
      code_buddy: "青岚",
      claude_code: "远山",
      gemini: "流光",
      cline: "逐风",
      kimi_code: "月白",
      pi: "墨川",
    })

    const presented = presentAgentSdkAgents(agents, (name) => `${name} SDK`)
    const names = Object.fromEntries(
      presented.map((agent) => [agent.agent_type, agent.name])
    )
    expect(names).toMatchObject({
      codex: "星河",
      open_code: "云舟",
      code_buddy: "青岚",
      claude_code: "远山",
      gemini: "流光",
      cline: "逐风",
      kimi_code: "月白",
      pi: "墨川",
      hermes: "Hermes Agent",
      open_claw: "OpenClaw",
    })
  })

  it("replaces branded descriptions only for aliased agents", () => {
    const presented = presentAgentSdkAgents(agents, (name) => `${name} SDK`)
    const byType = new Map(
      presented.map((agent) => [agent.agent_type, agent.description])
    )

    expect(byType.get("codex")).toBe("星河 SDK")
    expect(byType.get("hermes")).toBe("Hermes Agent description")
    expect(byType.get("open_claw")).toBe("OpenClaw description")
  })

  it("masks visible brand names without changing technical identifiers", () => {
    expect(
      maskAgentSdkBrandText(
        "Codex CLI, OpenCode, CodeBuddy, Claude Code, Gemini CLI, Cline, Kimi Code, Pi"
      )
    ).toBe("星河, 云舟, 青岚, 远山, 流光, 逐风, 月白, 墨川")

    const technical =
      "GEMINI_API_KEY opencode.json CODEBUDDY_API_KEY ~/.cline/data ~/.pi/agent gemini"
    expect(maskAgentSdkBrandText(technical)).toBe(technical)
  })
})
