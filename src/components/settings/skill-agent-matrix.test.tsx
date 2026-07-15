import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { Sparkles } from "lucide-react"

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}))

const mcpApiMocks = vi.hoisted(() => ({
  expertsList: vi.fn(),
  expertsOpenCentralDir: vi.fn(),
  expertsReadContent: vi.fn(),
  managedSkillsGetFamilyState: vi.fn(),
  managedSkillsGetGlobalState: vi.fn(),
  managedSkillsSetGlobalEnabled: vi.fn(),
  managedSkillsSetSkillEnabled: vi.fn(),
  mcpGetMarketplaceServerDetail: vi.fn(),
  mcpInstallFromMarketplace: vi.fn(),
  mcpListMarketplaces: vi.fn(),
  mcpRemoveServer: vi.fn(),
  mcpScanLocal: vi.fn(),
  mcpSearchMarketplace: vi.fn(),
  mcpSetServerEnabled: vi.fn(),
  mcpUpsertLocalServer: vi.fn(),
  officecliDetect: vi.fn(),
  officecliInstall: vi.fn(),
  officecliListSkills: vi.fn(),
  officecliSkillReadContent: vi.fn(),
  officecliSyncSkills: vi.fn(),
  officecliUninstall: vi.fn(),
  openFolder: vi.fn(),
}))

vi.mock("@/lib/api", () => mcpApiMocks)

import { toast } from "sonner"
import enMessages from "@/i18n/messages/en.json"
import {
  SkillAgentMatrix,
  computeLinkDelta,
  statusKey,
  type MatrixSkill,
  type SkillAgentMatrixProps,
} from "./skill-agent-matrix"
import { SkillToggleList, type SkillToggleListProps } from "./skill-toggle-list"
import { McpSettings } from "./mcp-settings"
import { ExpertsSettings } from "./experts-settings"
import { OfficeToolsSettings } from "./office-tools-settings"
import type {
  AcpAgentInfo,
  AgentType,
  ExpertInstallStatus,
  ExpertLinkState,
  ManagedSkillSyncReport,
} from "@/lib/types"

function makeStatus(
  expertId: string,
  agentType: AgentType,
  state: ExpertLinkState
): ExpertInstallStatus {
  return {
    expertId,
    agentType,
    state,
    linkPath: "",
    targetPath: null,
    expectedTargetPath: "",
    copyMode: false,
  }
}

function makeMap(
  statuses: ExpertInstallStatus[]
): Map<string, ExpertInstallStatus> {
  const m = new Map<string, ExpertInstallStatus>()
  for (const s of statuses) m.set(statusKey(s.expertId, s.agentType), s)
  return m
}

const enableable = () => true

describe("computeLinkDelta", () => {
  it("emits only cells that actually change when enabling", () => {
    const statuses = makeMap([
      makeStatus("a", "claude_code", "linked_to_iyw_claw"), // already on
      makeStatus("a", "codex", "not_linked"), // needs enabling
    ])
    const ops = computeLinkDelta(
      [
        { skillId: "a", agentType: "claude_code" },
        { skillId: "a", agentType: "codex" },
      ],
      true,
      statuses,
      enableable
    )
    expect(ops).toEqual([{ expertId: "a", agentType: "codex", enable: true }])
  })

  it("returns [] when nothing needs to change (idempotent)", () => {
    const statuses = makeMap([makeStatus("a", "codex", "linked_to_iyw_claw")])
    expect(
      computeLinkDelta(
        [{ skillId: "a", agentType: "codex" }],
        true,
        statuses,
        enableable
      )
    ).toEqual([])
  })

  it("skips not-ready skills and blocked cells when enabling", () => {
    const statuses = makeMap([
      makeStatus("ready", "codex", "not_linked"),
      makeStatus("blocked", "codex", "blocked_by_real_directory"),
      makeStatus("foreign", "codex", "linked_elsewhere"),
    ])
    const isReady = (id: string) => id !== "notsynced"
    const ops = computeLinkDelta(
      [
        { skillId: "ready", agentType: "codex" },
        { skillId: "blocked", agentType: "codex" },
        { skillId: "foreign", agentType: "codex" },
        { skillId: "notsynced", agentType: "codex" },
      ],
      true,
      statuses,
      isReady
    )
    expect(ops).toEqual([
      { expertId: "ready", agentType: "codex", enable: true },
    ])
  })

  it("disabling only emits currently-enabled cells", () => {
    const statuses = makeMap([
      makeStatus("a", "claude_code", "linked_to_iyw_claw"),
      makeStatus("a", "codex", "not_linked"),
      makeStatus("a", "gemini", "blocked_by_real_directory"),
    ])
    const ops = computeLinkDelta(
      [
        { skillId: "a", agentType: "claude_code" },
        { skillId: "a", agentType: "codex" },
        { skillId: "a", agentType: "gemini" },
      ],
      false,
      statuses,
      enableable
    )
    expect(ops).toEqual([
      { expertId: "a", agentType: "claude_code", enable: false },
    ])
  })
})

// ─── Component ─────────────────────────────────────────────────────────

function agent(agentType: AgentType, name: string): AcpAgentInfo {
  return { agent_type: agentType, name } as unknown as AcpAgentInfo
}

const SKILLS: MatrixSkill[] = [
  {
    id: "brainstorming",
    category: "discovery",
    displayName: "Brainstorming",
    description: "desc",
    icon: Sparkles,
    ready: true,
  },
]

const AGENTS = [agent("claude_code", "Claude Code"), agent("codex", "Codex")]

function renderMatrix(overrides: Partial<SkillAgentMatrixProps> = {}) {
  const props: SkillAgentMatrixProps = {
    skills: SKILLS,
    agents: AGENTS,
    categoryOrder: { discovery: 1 },
    translateCategory: (c) => c,
    translateState: (s) => s,
    loadAllStatuses: vi
      .fn()
      .mockResolvedValue([
        makeStatus("brainstorming", "claude_code", "not_linked"),
        makeStatus("brainstorming", "codex", "not_linked"),
      ]),
    applyLinks: vi.fn().mockResolvedValue([
      {
        expertId: "brainstorming",
        agentType: "claude_code",
        ok: true,
        status: makeStatus(
          "brainstorming",
          "claude_code",
          "linked_to_iyw_claw"
        ),
        error: null,
      },
    ]),
    loadContent: vi.fn().mockResolvedValue("# Brainstorming"),
    ...overrides,
  }
  render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      <SkillAgentMatrix {...props} />
    </NextIntlClientProvider>
  )
  return props
}

describe("SkillAgentMatrix", () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it("toggles a single cell through applyLinks then reconciles", async () => {
    const props = renderMatrix()
    const cell = await screen.findByRole("switch", {
      name: "Brainstorming, Claude Code: not_linked",
    })
    fireEvent.click(cell)

    await waitFor(() => {
      expect(props.applyLinks).toHaveBeenCalledTimes(1)
    })
    expect(props.applyLinks).toHaveBeenCalledWith([
      { expertId: "brainstorming", agentType: "claude_code", enable: true },
    ])
    // Mount load + post-batch reconcile.
    await waitFor(() => {
      expect(props.loadAllStatuses).toHaveBeenCalledTimes(2)
    })
    expect(toast.success).toHaveBeenCalled()
  })

  it("notifies onApplied with the touched agents", async () => {
    const onApplied = vi.fn()
    renderMatrix({ onApplied })
    const cell = await screen.findByRole("switch", {
      name: "Brainstorming, Claude Code: not_linked",
    })
    fireEvent.click(cell)
    await waitFor(() => {
      expect(onApplied).toHaveBeenCalledWith(["claude_code"])
    })
  })

  it("shows a single partial-failure toast when an op fails", async () => {
    const applyLinks = vi.fn().mockResolvedValue([
      {
        expertId: "brainstorming",
        agentType: "claude_code",
        ok: false,
        status: null,
        error: "name collision",
      },
    ])
    renderMatrix({ applyLinks })
    const cell = await screen.findByRole("switch", {
      name: "Brainstorming, Claude Code: not_linked",
    })
    fireEvent.click(cell)
    await waitFor(() => {
      expect(toast.warning).toHaveBeenCalledTimes(1)
    })
    expect(toast.success).not.toHaveBeenCalled()
  })

  it("does not toggle a blocked cell", async () => {
    const applyLinks = vi.fn()
    renderMatrix({
      applyLinks,
      loadAllStatuses: vi
        .fn()
        .mockResolvedValue([
          makeStatus(
            "brainstorming",
            "claude_code",
            "blocked_by_real_directory"
          ),
          makeStatus("brainstorming", "codex", "not_linked"),
        ]),
    })
    const cell = await screen.findByRole("switch", {
      name: "Brainstorming, Claude Code: blocked_by_real_directory",
    })
    expect(cell).toBeDisabled()
    fireEvent.click(cell)
    expect(applyLinks).not.toHaveBeenCalled()
  })

  it("disables cells for a not-ready (un-synced) skill", async () => {
    const applyLinks = vi.fn()
    renderMatrix({
      applyLinks,
      skills: [{ ...SKILLS[0], ready: false }],
    })
    const cell = await screen.findByRole("switch", {
      name: "Brainstorming, Claude Code: not_linked",
    })
    expect(cell).toBeDisabled()
    fireEvent.click(cell)
    expect(applyLinks).not.toHaveBeenCalled()
  })
})

describe("SkillToggleList global policy", () => {
  it("persists one global switch without requiring per-agent targets", async () => {
    const setGlobalEnabled = vi.fn().mockResolvedValue({
      family: "experts",
      enabled: true,
      results: [],
      touchedAgents: ["codex"],
    })
    const onApplied = vi.fn()

    render(
      <NextIntlClientProvider locale="en" messages={enMessages}>
        <SkillToggleList
          skills={[
            {
              id: "brainstorming",
              category: "discovery",
              displayName: "Brainstorming",
              description: "desc",
              ready: true,
            },
          ]}
          skillStates={[
            { skillId: "brainstorming", enabled: false, ready: true },
          ]}
          globalEnabled={false}
          setGlobalEnabled={setGlobalEnabled}
          setSkillEnabled={vi.fn()}
          categoryOrder={{ discovery: 1 }}
          translateCategory={(category) => category}
          onApplied={onApplied}
        />
      </NextIntlClientProvider>
    )

    const toggle = screen.getByRole("switch", { name: /enable all skills/i })
    expect(toggle).not.toBeChecked()
    fireEvent.click(toggle)

    await waitFor(() => {
      expect(setGlobalEnabled).toHaveBeenCalledWith(true)
      expect(toggle).toBeChecked()
    })
    expect(onApplied).toHaveBeenCalledWith(["codex"])
  })

  it("allows an enabled policy to be turned off before skills are ready", () => {
    render(
      <NextIntlClientProvider locale="en" messages={enMessages}>
        <SkillToggleList
          skills={[
            {
              id: "officecli-docx",
              category: "documents",
              displayName: "Documents",
              description: "desc",
              ready: false,
            },
          ]}
          skillStates={[
            { skillId: "officecli-docx", enabled: true, ready: false },
          ]}
          globalEnabled
          setGlobalEnabled={vi.fn()}
          setSkillEnabled={vi.fn()}
          categoryOrder={{ documents: 1 }}
          translateCategory={(category) => category}
        />
      </NextIntlClientProvider>
    )

    expect(
      screen.getByRole("switch", { name: /enable all skills/i })
    ).toBeEnabled()
  })
})

const PER_SKILL_ITEMS = [
  {
    id: "brainstorming",
    category: "discovery",
    displayName: "Brainstorming",
    description: "desc",
    ready: true,
  },
  {
    id: "executing-plans",
    category: "execution",
    displayName: "Executing Plans",
    description: "desc",
    ready: true,
  },
]

const PER_SKILL_STATES = [
  { skillId: "brainstorming", enabled: false, ready: true },
  { skillId: "executing-plans", enabled: false, ready: true },
]

function renderPerSkillList(
  setSkillEnabled: SkillToggleListProps["setSkillEnabled"],
  skillStates = PER_SKILL_STATES,
  skills = PER_SKILL_ITEMS
) {
  render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      <SkillToggleList
        skills={skills}
        skillStates={skillStates}
        globalEnabled={false}
        setGlobalEnabled={vi.fn()}
        setSkillEnabled={setSkillEnabled}
        categoryOrder={{ discovery: 1, execution: 2 }}
        translateCategory={(category) => category}
      />
    </NextIntlClientProvider>
  )
}

describe("SkillToggleList per-skill policy", () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it("toggles one skill without changing its sibling", async () => {
    const setSkillEnabled = vi.fn().mockResolvedValue({
      family: "experts",
      enabled: true,
      skillId: "brainstorming",
      results: [],
      touchedAgents: ["codex"],
    })

    renderPerSkillList(setSkillEnabled)

    const brainstorming = screen.getByRole("switch", {
      name: "Brainstorming",
    })
    const executingPlans = screen.getByRole("switch", {
      name: "Executing Plans",
    })

    fireEvent.click(brainstorming)

    await waitFor(() => {
      expect(setSkillEnabled).toHaveBeenCalledWith("brainstorming", true)
      expect(brainstorming).toBeChecked()
    })
    expect(executingPlans).not.toBeChecked()
  })

  it("disables only the skill with a pending request", async () => {
    let resolveBrainstorming!: (value: ManagedSkillSyncReport) => void
    const brainstormingRequest = new Promise<ManagedSkillSyncReport>(
      (resolve) => {
        resolveBrainstorming = resolve
      }
    )
    const setSkillEnabled = vi
      .fn<SkillToggleListProps["setSkillEnabled"]>()
      .mockImplementation((skillId) =>
        skillId === "brainstorming"
          ? brainstormingRequest
          : Promise.resolve({
              family: "experts",
              enabled: true,
              skillId,
              results: [],
              touchedAgents: [],
            })
      )
    renderPerSkillList(setSkillEnabled)
    const brainstorming = screen.getByRole("switch", {
      name: "Brainstorming",
    })
    const executingPlans = screen.getByRole("switch", {
      name: "Executing Plans",
    })

    fireEvent.click(brainstorming)

    await waitFor(() => expect(brainstorming).toBeDisabled())
    expect(executingPlans).toBeEnabled()
    fireEvent.click(executingPlans)
    await waitFor(() => {
      expect(setSkillEnabled).toHaveBeenCalledWith("executing-plans", true)
      expect(executingPlans).toBeChecked()
    })

    resolveBrainstorming({
      family: "experts",
      enabled: true,
      skillId: "brainstorming",
      results: [],
      touchedAgents: [],
    })
    await waitFor(() => expect(brainstorming).toBeEnabled())
  })

  it("restores only the failed skill after a rejected request", async () => {
    const setSkillEnabled = vi.fn().mockRejectedValue(new Error("offline"))
    renderPerSkillList(setSkillEnabled, [
      { skillId: "brainstorming", enabled: false, ready: true },
      { skillId: "executing-plans", enabled: true, ready: true },
    ])
    const brainstorming = screen.getByRole("switch", {
      name: "Brainstorming",
    })
    const executingPlans = screen.getByRole("switch", {
      name: "Executing Plans",
    })

    fireEvent.click(brainstorming)

    await waitFor(() => {
      expect(setSkillEnabled).toHaveBeenCalledWith("brainstorming", true)
      expect(brainstorming).not.toBeChecked()
      expect(brainstorming).toBeEnabled()
    })
    expect(executingPlans).toBeChecked()
    expect(executingPlans).toBeEnabled()
    expect(toast.error).toHaveBeenCalledTimes(1)
  })

  it("warns when a skill policy is only partially applied", async () => {
    const setSkillEnabled = vi.fn().mockResolvedValue({
      family: "experts",
      enabled: true,
      skillId: "brainstorming",
      results: [
        {
          expertId: "brainstorming",
          agentType: "codex",
          ok: false,
          status: null,
          error: "name collision",
        },
      ],
      touchedAgents: [],
    })
    renderPerSkillList(setSkillEnabled)
    const brainstorming = screen.getByRole("switch", {
      name: "Brainstorming",
    })

    fireEvent.click(brainstorming)

    await waitFor(() => {
      expect(brainstorming).toBeChecked()
      expect(toast.warning).toHaveBeenCalledTimes(1)
    })
    expect(toast.error).not.toHaveBeenCalled()
  })

  it("allows a desired-on not-ready skill to be disabled", async () => {
    const setSkillEnabled = vi.fn().mockResolvedValue({
      family: "experts",
      enabled: false,
      skillId: "brainstorming",
      results: [],
      touchedAgents: [],
    })
    renderPerSkillList(
      setSkillEnabled,
      [{ skillId: "brainstorming", enabled: true, ready: false }],
      [{ ...PER_SKILL_ITEMS[0], ready: false }]
    )
    const toggle = screen.getByRole("switch", { name: "Brainstorming" })

    expect(toggle).toBeChecked()
    expect(toggle).toBeEnabled()
    fireEvent.click(toggle)

    await waitFor(() => {
      expect(setSkillEnabled).toHaveBeenCalledWith("brainstorming", false)
      expect(toggle).not.toBeChecked()
      expect(toggle).toBeDisabled()
    })
  })
})

describe("managed skill settings family routing", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mcpApiMocks.managedSkillsGetGlobalState.mockResolvedValue({
      expertsEnabled: false,
      officeToolsEnabled: false,
    })
    mcpApiMocks.managedSkillsGetFamilyState.mockImplementation((family) =>
      Promise.resolve({
        family,
        allEnabled: false,
        skills:
          family === "experts"
            ? [
                { skillId: "brainstorming", enabled: false, ready: true },
                { skillId: "executing-plans", enabled: false, ready: true },
              ]
            : [
                { skillId: "officecli-docx", enabled: false, ready: true },
                { skillId: "officecli-xlsx", enabled: false, ready: true },
              ],
      })
    )
    mcpApiMocks.managedSkillsSetSkillEnabled.mockImplementation(
      (family, skillId, enabled) =>
        Promise.resolve({
          family,
          enabled,
          skillId,
          results: [],
          touchedAgents: [],
        })
    )
    mcpApiMocks.expertsList.mockResolvedValue([
      {
        metadata: {
          id: "brainstorming",
          category: "discovery",
          icon: null,
          sort_order: 1,
          display_name: { en: "Brainstorming" },
          description: { en: "desc" },
          bundled_hash: "hash-a",
        },
        installed_centrally: true,
        user_modified: false,
        central_path: "C:/skills/brainstorming",
      },
      {
        metadata: {
          id: "executing-plans",
          category: "execution",
          icon: null,
          sort_order: 2,
          display_name: { en: "Executing Plans" },
          description: { en: "desc" },
          bundled_hash: "hash-b",
        },
        installed_centrally: true,
        user_modified: false,
        central_path: "C:/skills/executing-plans",
      },
    ])
    mcpApiMocks.expertsReadContent.mockResolvedValue("# Skill")
    mcpApiMocks.officecliDetect.mockResolvedValue({
      installed: true,
      version: "1.0.0",
      path: "C:/officecli.exe",
      runtimeError: null,
    })
    mcpApiMocks.officecliListSkills.mockResolvedValue([
      {
        id: "officecli-docx",
        category: "documents",
        icon: "file-text",
        sortOrder: 1,
        displayName: { en: "Documents" },
        description: { en: "desc" },
        installedCentrally: true,
      },
      {
        id: "officecli-xlsx",
        category: "spreadsheets",
        icon: "sheet",
        sortOrder: 2,
        displayName: { en: "Spreadsheets" },
        description: { en: "desc" },
        installedCentrally: true,
      },
    ])
  })

  it("routes an expert row toggle through the experts family", async () => {
    render(
      <NextIntlClientProvider locale="en" messages={enMessages}>
        <ExpertsSettings />
      </NextIntlClientProvider>
    )

    const toggle = await screen.findByRole("switch", {
      name: "Brainstorming",
    })
    expect(mcpApiMocks.managedSkillsGetFamilyState).toHaveBeenCalledWith(
      "experts"
    )
    const enableAll = screen.getByRole("switch", {
      name: /enable all skills/i,
    })
    expect(enableAll).not.toBeChecked()
    fireEvent.click(toggle)

    await waitFor(() => {
      expect(mcpApiMocks.managedSkillsSetSkillEnabled).toHaveBeenCalledWith(
        "experts",
        "brainstorming",
        true
      )
    })
    expect(enableAll).not.toBeChecked()

    fireEvent.click(screen.getByRole("switch", { name: "Executing Plans" }))
    await waitFor(() => {
      expect(mcpApiMocks.managedSkillsSetSkillEnabled).toHaveBeenCalledWith(
        "experts",
        "executing-plans",
        true
      )
      expect(enableAll).toBeChecked()
    })
  })

  it("routes an Office row toggle through the office family", async () => {
    render(
      <NextIntlClientProvider locale="en" messages={enMessages}>
        <OfficeToolsSettings />
      </NextIntlClientProvider>
    )

    const toggle = await screen.findByRole("switch", { name: "Documents" })
    expect(mcpApiMocks.managedSkillsGetFamilyState).toHaveBeenCalledWith(
      "office_tools"
    )
    fireEvent.click(toggle)

    await waitFor(() => {
      expect(mcpApiMocks.managedSkillsSetSkillEnabled).toHaveBeenCalledWith(
        "office_tools",
        "officecli-docx",
        true
      )
    })
  })
})

describe("McpSettings global distribution", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mcpApiMocks.mcpListMarketplaces.mockResolvedValue([])
    mcpApiMocks.mcpScanLocal.mockResolvedValue([
      {
        id: "filesystem",
        spec: { type: "stdio", command: "npx" },
        apps: ["codex"],
        enabled: true,
      },
    ])
    mcpApiMocks.mcpSetServerEnabled.mockResolvedValue(null)
  })

  it("hides target-app selection and keeps one switch per MCP", async () => {
    render(
      <NextIntlClientProvider locale="en" messages={enMessages}>
        <McpSettings />
      </NextIntlClientProvider>
    )

    const toggle = await screen.findByRole("switch", { name: "filesystem" })
    expect(toggle).toBeChecked()
    expect(screen.queryByText("Enabled Apps")).not.toBeInTheDocument()

    fireEvent.click(toggle)

    await waitFor(() => {
      expect(mcpApiMocks.mcpSetServerEnabled).toHaveBeenCalledWith(
        "filesystem",
        false
      )
    })
  })
})
