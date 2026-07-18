import { describe, expect, it } from "vitest"
import {
  AGENT_PROFILE_MESSAGE_KEYS,
  getAgentVersionState,
  needsManagedRuntimePreparation,
} from "@/lib/agent-sdk-overview"
import { ALL_AGENT_TYPES, type AcpAgentInfo } from "@/lib/types"

function createAgent(overrides: Partial<AcpAgentInfo> = {}): AcpAgentInfo {
  return {
    agent_type: "codex",
    registry_id: "codex-acp",
    registry_version: "1.2.0",
    name: "星河",
    description: "",
    available: true,
    distribution_type: "npx",
    enabled: true,
    sort_order: 0,
    installed_version: "1.2.0",
    env: {},
    config_json: null,
    config_file_path: null,
    opencode_auth_json: null,
    codex_auth_json: null,
    codex_config_toml: null,
    cline_secrets_json: null,
    hermes_config_yaml: null,
    model_provider_id: null,
    ...overrides,
  }
}

describe("Agent SDK overview presentation", () => {
  it("defines a description and three strengths for every Agent", () => {
    expect(Object.keys(AGENT_PROFILE_MESSAGE_KEYS).sort()).toEqual(
      [...ALL_AGENT_TYPES].sort()
    )

    for (const profile of Object.values(AGENT_PROFILE_MESSAGE_KEYS)) {
      expect(profile.description).toMatch(/^profiles\./)
      expect(profile.strengths).toHaveLength(3)
    }
  })

  it.each([
    [createAgent({ installed_version: null }), "notInstalled"],
    [createAgent({ installed_version: "1.1.0" }), "upgradeAvailable"],
    [createAgent(), "latest"],
    [
      createAgent({ installed_version: "dev", registry_version: "1.2.0" }),
      "unknown",
    ],
    [
      createAgent({ installed_version: "1.2.0", registry_version: null }),
      "unknown",
    ],
    [
      createAgent({
        available: false,
        distribution_type: "binary",
        installed_version: null,
      }),
      "unsupported",
    ],
  ] as const)("maps version data to %s", (agent, expected) => {
    expect(getAgentVersionState(agent)).toBe(expected)
  })

  it("prepares a uvx runtime inside the Agent install flow", () => {
    const hermes = createAgent({
      agent_type: "hermes",
      distribution_type: "uvx",
      installed_version: null,
    })

    expect(
      needsManagedRuntimePreparation(hermes, [
        { check_id: "uv_available", status: "fail" },
      ])
    ).toBe(true)
    expect(
      needsManagedRuntimePreparation(hermes, [
        { check_id: "uv_available", status: "pass" },
      ])
    ).toBe(false)
    expect(needsManagedRuntimePreparation(createAgent(), [])).toBe(false)
  })
})
