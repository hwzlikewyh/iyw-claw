import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { useState } from "react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { AgentStorageStatus } from "@/lib/types"

const validateAgentStorageRoot = vi.fn()
const migrateAgentStorage = vi.fn()

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string, values?: Record<string, string>) =>
    values?.path ? `${key}:${values.path}` : key,
}))

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}))

vi.mock("@/lib/api", () => ({
  validateAgentStorageRoot: (...args: unknown[]) =>
    validateAgentStorageRoot(...args),
  initializeAgentStorage: vi.fn(),
  migrateAgentStorage: (...args: unknown[]) => migrateAgentStorage(...args),
  updateAgentProfileOverride: vi.fn(),
}))

vi.mock("@/lib/platform", () => ({ openFileDialog: vi.fn() }))
vi.mock("@/lib/updater", () => ({ relaunchApp: vi.fn() }))

import { AgentStorageSettings } from "./agent-storage-settings"

const initialized: AgentStorageStatus = {
  initialized: true,
  activeRoot: "D:/Apps/iyw-claw-data",
  suggestedRoot: null,
  allowSystemDrive: false,
  restartRequired: false,
  profilePaths: [],
  previousRoot: null,
}

describe("AgentStorageSettings migration", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    validateAgentStorageRoot.mockImplementation(async (root: string) => ({
      absolutePath: root,
      writable: true,
      onSystemDrive: false,
      error: null,
    }))
    migrateAgentStorage.mockResolvedValue({
      ...initialized,
      activeRoot: "E:/AgentData",
      previousRoot: "D:/Apps/iyw-claw-data",
      restartRequired: true,
    })
  })

  it("reports the untouched previous root after migration", async () => {
    function Harness() {
      const [status, setStatus] = useState(initialized)
      return (
        <AgentStorageSettings
          status={status}
          selectedAgent={{ agentType: "codex", name: "Codex" }}
          onStatusChange={setStatus}
        />
      )
    }
    render(<Harness />)

    fireEvent.change(screen.getByLabelText("storage.migrationPathLabel"), {
      target: { value: "E:/AgentData" },
    })
    fireEvent.click(screen.getByRole("button", { name: "storage.migrate" }))

    await waitFor(() =>
      expect(migrateAgentStorage).toHaveBeenCalledWith({
        root: "E:/AgentData",
        allowSystemDrive: false,
      })
    )
    expect(
      await screen.findByText("storage.previousRoot:D:/Apps/iyw-claw-data")
    ).toBeInTheDocument()
  })
})
