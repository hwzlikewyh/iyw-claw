import { render, screen, waitFor, fireEvent } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"

import zhMessages from "@/i18n/messages/zh-CN.json"
import { InternetToolsSettings } from "./internet-tools-settings"

const api = vi.hoisted(() => ({
  internetToolsDetect: vi.fn(),
  internetToolInstall: vi.fn(),
  internetToolUninstall: vi.fn(),
  internetToolsSyncSkills: vi.fn(),
  internetToolsListSkills: vi.fn(),
  internetToolsReadSkill: vi.fn(),
  internetToolsAgentReachDoctor: vi.fn(),
  internetToolsOpencliDoctor: vi.fn(),
  internetToolsConfigureAgentReach: vi.fn(),
  internetToolsImportBrowser: vi.fn(),
  internetToolsInstallChannels: vi.fn(),
  managedSkillsGetGlobalState: vi.fn(),
  managedSkillsGetFamilyState: vi.fn(),
  managedSkillsReconcileFamily: vi.fn(),
  managedSkillsSetGlobalEnabled: vi.fn(),
  managedSkillsSetSkillEnabled: vi.fn(),
}))

vi.mock("@/lib/api", () => api)
vi.mock("@/lib/platform", () => ({ openUrl: vi.fn() }))

function renderSettings() {
  return render(
    <NextIntlClientProvider locale="zh-CN" messages={zhMessages}>
      <InternetToolsSettings />
    </NextIntlClientProvider>
  )
}

describe("InternetToolsSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    api.internetToolsDetect.mockResolvedValue([
      {
        id: "agent_reach",
        status: "installed",
        installed: true,
        version: "1.5.0",
        expectedVersion: "1.5.0",
        path: "C:/agent-reach.exe",
        runtimeError: null,
      },
      {
        id: "opencli",
        status: "not_installed",
        installed: false,
        version: null,
        expectedVersion: "1.8.6",
        path: null,
        runtimeError: null,
      },
    ])
    api.internetToolsListSkills.mockResolvedValue([])
    api.managedSkillsReconcileFamily.mockResolvedValue({
      family: "internet_tools",
      allEnabled: false,
      skills: [],
      agents: [],
    })
    api.managedSkillsGetGlobalState.mockResolvedValue({
      expertsEnabled: false,
      officeToolsEnabled: false,
      internetToolsEnabled: false,
    })
    api.managedSkillsGetFamilyState.mockResolvedValue({
      family: "internet_tools",
      allEnabled: false,
      skills: [],
    })
    api.internetToolsAgentReachDoctor.mockResolvedValue([
      {
        id: "github",
        status: "ok",
        name: "GitHub 仓库和代码",
        message: "完整可用",
        tier: 0,
        backends: ["gh CLI"],
        activeBackend: "gh CLI",
      },
    ])
  })

  it("shows Agent Reach and OpenCLI management cards", async () => {
    renderSettings()

    expect(await screen.findByText("联网工具")).toBeInTheDocument()
    expect(screen.getByText("Agent Reach")).toBeInTheDocument()
    expect(screen.getByText("OpenCLI")).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "安装" })).toBeInTheDocument()
  })

  it("runs Agent Reach diagnostics and displays the active backend", async () => {
    renderSettings()
    const doctor = await screen.findByRole("button", { name: "运行诊断" })

    fireEvent.click(doctor)

    await waitFor(() =>
      expect(api.internetToolsAgentReachDoctor).toHaveBeenCalledTimes(1)
    )
    expect(await screen.findByText("GitHub 仓库和代码")).toBeInTheDocument()
    expect(screen.getByText(/gh CLI/)).toBeInTheDocument()
  })

  it("reapplies the saved skill policy after installing a tool", async () => {
    api.internetToolInstall.mockResolvedValue({})
    renderSettings()

    fireEvent.click(await screen.findByRole("button", { name: "安装" }))

    await waitFor(() =>
      expect(api.managedSkillsReconcileFamily).toHaveBeenCalledWith(
        "internet_tools"
      )
    )
  })
})
