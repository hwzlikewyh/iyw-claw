import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { AgentStorageStatus } from "@/lib/types"

const validateAgentStorageRoot = vi.fn()
const initializeAgentStorage = vi.fn()
const migrateAgentStorage = vi.fn()
const updateAgentProfileOverride = vi.fn()
const relaunchApp = vi.fn()
const openFileDialog = vi.fn()

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string, values?: Record<string, string>) =>
    values?.name
      ? `${key}:${values.name}`
      : values?.path
        ? `${key}:${values.path}`
        : key,
}))

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}))

vi.mock("@/lib/api", () => ({
  validateAgentStorageRoot: (...args: unknown[]) =>
    validateAgentStorageRoot(...args),
  initializeAgentStorage: (...args: unknown[]) =>
    initializeAgentStorage(...args),
  migrateAgentStorage: (...args: unknown[]) => migrateAgentStorage(...args),
  updateAgentProfileOverride: (...args: unknown[]) =>
    updateAgentProfileOverride(...args),
}))

vi.mock("@/lib/platform", () => ({
  openFileDialog: (...args: unknown[]) => openFileDialog(...args),
}))

vi.mock("@/lib/updater", () => ({
  relaunchApp: (...args: unknown[]) => relaunchApp(...args),
}))

import { AgentStorageSettings } from "./agent-storage-settings"

const uninitialized: AgentStorageStatus = {
  initialized: false,
  activeRoot: null,
  suggestedRoot: "D:/Apps/iyw-claw-data",
  allowSystemDrive: false,
  restartRequired: false,
  profilePaths: [],
  previousRoot: null,
}

const initialized: AgentStorageStatus = {
  initialized: true,
  activeRoot: "D:/Apps/iyw-claw-data",
  suggestedRoot: null,
  allowSystemDrive: false,
  restartRequired: false,
  profilePaths: [
    {
      agentType: "codex",
      path: "D:/Apps/iyw-claw-data/config/codex",
      overridden: false,
    },
  ],
  previousRoot: null,
}

describe("AgentStorageSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    validateAgentStorageRoot.mockImplementation(async (root: string) => ({
      absolutePath: root,
      writable: true,
      onSystemDrive: false,
      error: null,
    }))
    initializeAgentStorage.mockResolvedValue({
      ...initialized,
      restartRequired: true,
    })
  })

  it("initializes the suggested private root without opening a custom install flow", async () => {
    const onStatusChange = vi.fn()
    render(
      <AgentStorageSettings
        status={uninitialized}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={onStatusChange}
      />
    )

    expect(screen.getByLabelText("storage.rootLabel")).toHaveValue(
      "D:/Apps/iyw-claw-data"
    )
    fireEvent.click(screen.getByRole("button", { name: "storage.initialize" }))

    await waitFor(() =>
      expect(initializeAgentStorage).toHaveBeenCalledWith({
        root: "D:/Apps/iyw-claw-data",
        allowSystemDrive: false,
        importExistingSettings: true,
      })
    )
    expect(onStatusChange).toHaveBeenCalled()
  })

  it("requires explicit confirmation before initializing on the system drive", async () => {
    validateAgentStorageRoot.mockResolvedValue({
      absolutePath: "C:/iyw-claw-data",
      writable: true,
      onSystemDrive: true,
      error: null,
    })
    render(
      <AgentStorageSettings
        status={{ ...uninitialized, suggestedRoot: "C:/iyw-claw-data" }}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={vi.fn()}
      />
    )

    fireEvent.click(screen.getByRole("button", { name: "storage.initialize" }))
    expect(
      await screen.findByText("storage.systemDriveWarning")
    ).toBeInTheDocument()
    expect(initializeAgentStorage).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole("button", { name: "storage.confirm" }))
    await waitFor(() =>
      expect(initializeAgentStorage).toHaveBeenCalledWith({
        root: "C:/iyw-claw-data",
        allowSystemDrive: true,
        importExistingSettings: true,
      })
    )
  })

  it("lets the user choose the storage directory and disable one-time import", async () => {
    openFileDialog.mockResolvedValue("E:/AgentData")
    render(
      <AgentStorageSettings
        status={uninitialized}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={vi.fn()}
      />
    )

    fireEvent.click(
      screen.getByRole("button", { name: "storage.chooseDirectory" })
    )
    expect(await screen.findByDisplayValue("E:/AgentData")).toBeInTheDocument()
    fireEvent.click(screen.getByRole("checkbox"))
    fireEvent.click(screen.getByRole("button", { name: "storage.initialize" }))

    await waitFor(() =>
      expect(initializeAgentStorage).toHaveBeenCalledWith({
        root: "E:/AgentData",
        allowSystemDrive: false,
        importExistingSettings: false,
      })
    )
  })

  it("does not initialize an unwritable directory", async () => {
    validateAgentStorageRoot.mockResolvedValue({
      absolutePath: "D:/blocked",
      writable: false,
      onSystemDrive: false,
      error: "access denied",
    })
    render(
      <AgentStorageSettings
        status={{ ...uninitialized, suggestedRoot: "D:/blocked" }}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={vi.fn()}
      />
    )

    fireEvent.click(screen.getByRole("button", { name: "storage.initialize" }))
    await waitFor(() => expect(validateAgentStorageRoot).toHaveBeenCalled())
    expect(initializeAgentStorage).not.toHaveBeenCalled()
  })

  it("saves and resets the selected Agent profile directory", async () => {
    updateAgentProfileOverride.mockResolvedValue({
      ...initialized,
      restartRequired: true,
      profilePaths: [
        {
          agentType: "codex",
          path: "D:/Profiles/codex-private",
          overridden: true,
        },
      ],
    })
    render(
      <AgentStorageSettings
        status={{
          ...initialized,
          profilePaths: initialized.profilePaths.map((profile) => ({
            ...profile,
            overridden: true,
          })),
        }}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={vi.fn()}
      />
    )

    const input = screen.getByLabelText("storage.profilePathLabel:Codex")
    fireEvent.change(input, { target: { value: "D:/Profiles/codex-private" } })
    fireEvent.click(screen.getByRole("button", { name: "storage.saveProfile" }))

    await waitFor(() =>
      expect(updateAgentProfileOverride).toHaveBeenCalledWith({
        agentType: "codex",
        path: "D:/Profiles/codex-private",
        allowSystemDrive: false,
        allowUserGlobalProfile: false,
      })
    )

    fireEvent.click(
      screen.getByRole("button", { name: "storage.resetProfile" })
    )
    await waitFor(() =>
      expect(updateAgentProfileOverride).toHaveBeenCalledWith({
        agentType: "codex",
        path: null,
        allowSystemDrive: false,
        allowUserGlobalProfile: false,
      })
    )
  })

  it("requires a second confirmation before using the user global profile", async () => {
    updateAgentProfileOverride
      .mockRejectedValueOnce(
        new Error(
          "Using the existing user-global Agent profile requires explicit confirmation"
        )
      )
      .mockResolvedValueOnce({ ...initialized, restartRequired: true })
    render(
      <AgentStorageSettings
        status={initialized}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={vi.fn()}
      />
    )

    fireEvent.change(screen.getByLabelText("storage.profilePathLabel:Codex"), {
      target: { value: "C:/Users/demo/.codex" },
    })
    fireEvent.click(screen.getByRole("button", { name: "storage.saveProfile" }))

    expect(
      await screen.findByText("storage.globalProfileWarning")
    ).toBeInTheDocument()
    fireEvent.click(screen.getByRole("button", { name: "storage.confirm" }))

    await waitFor(() =>
      expect(updateAgentProfileOverride).toHaveBeenLastCalledWith({
        agentType: "codex",
        path: "C:/Users/demo/.codex",
        allowSystemDrive: false,
        allowUserGlobalProfile: true,
      })
    )
  })

  it("offers an immediate restart after a profile change", async () => {
    render(
      <AgentStorageSettings
        status={{ ...initialized, restartRequired: true }}
        selectedAgent={{ agentType: "codex", name: "Codex" }}
        onStatusChange={vi.fn()}
      />
    )

    fireEvent.click(screen.getByRole("button", { name: "storage.restartNow" }))
    await waitFor(() => expect(relaunchApp).toHaveBeenCalledTimes(1))
  })
})
