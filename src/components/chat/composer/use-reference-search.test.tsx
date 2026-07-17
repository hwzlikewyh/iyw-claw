import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { AcpAgentInfo, AgentType } from "@/lib/types"

import { useReferenceSearch } from "./use-reference-search"

const mocks = vi.hoisted(() => ({
  listAllConversations: vi.fn(),
}))

vi.mock("@/hooks/use-acp-agents", () => ({
  useAcpAgents: () => ({
    agents: [
      makeAgent("codex", "Codex CLI", "OpenAI Codex CLI integration"),
      makeAgent(
        "claude_code",
        "Claude Code",
        "Anthropic Claude Code integration"
      ),
      makeAgent("hermes", "Hermes Agent", "Hermes Agent integration"),
      makeAgent("open_claw", "OpenClaw", "OpenClaw integration"),
    ],
  }),
}))

vi.mock("@/hooks/use-file-tree", () => ({
  useFileTree: () => ({ allFiles: [], loaded: false }),
}))

vi.mock("@/lib/api", () => ({
  gitLog: vi.fn(),
  listAllConversations: mocks.listAllConversations,
}))

vi.mock("@/hooks/use-agent-sdk-translations", () => ({
  useAgentSdkTranslations: () => (key: string, values?: { name?: string }) =>
    key === "agentAliasDescription"
      ? `通过 Agent SDK 连接和管理${values?.name}。`
      : key,
}))

function makeAgent(
  agentType: AgentType,
  name: string,
  description: string
): AcpAgentInfo {
  return {
    agent_type: agentType,
    registry_id: agentType,
    registry_version: null,
    name,
    description,
    available: true,
    distribution_type: "binary",
    enabled: true,
    sort_order: 0,
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

describe("useReferenceSearch", () => {
  beforeEach(() => {
    mocks.listAllConversations.mockResolvedValue([])
  })

  it("uses neutral Agent SDK presentation in the mention menu", async () => {
    const { result } = renderHook(() =>
      useReferenceSearch({ defaultPath: null })
    )

    let groups: Awaited<ReturnType<typeof result.current>> | undefined
    await act(async () => {
      groups = await result.current("")
    })

    const agentItems = groups?.find((group) => group.kind === "agent")?.items
    expect(
      agentItems?.map((item) => ({
        id: item.reference.id,
        label: item.reference.label,
        detail: item.detail,
      }))
    ).toEqual([
      {
        id: "codex",
        label: "星河",
        detail: "通过 Agent SDK 连接和管理星河。",
      },
      {
        id: "claude_code",
        label: "远山",
        detail: "通过 Agent SDK 连接和管理远山。",
      },
      {
        id: "hermes",
        label: "Hermes Agent",
        detail: "Hermes Agent integration",
      },
      {
        id: "open_claw",
        label: "OpenClaw",
        detail: "OpenClaw integration",
      },
    ])
  })
})
