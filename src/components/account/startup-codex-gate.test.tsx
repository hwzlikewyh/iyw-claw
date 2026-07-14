import { act, render, screen, waitFor } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"

import enMessages from "@/i18n/messages/en.json"
import { StartupCodexGate } from "./startup-codex-gate"

const mocks = vi.hoisted(() => ({
  acpDetectAgentLocalVersion: vi.fn(),
  acpListAgents: vi.fn(),
  acpPrepareNpxAgent: vi.fn(),
  refreshAgents: vi.fn(),
}))

vi.mock("@/contexts/iyw-account-context", () => ({
  useIywAccount: () => ({ status: "authenticated" }),
}))

vi.mock("@/hooks/use-acp-agents", () => ({
  useAcpAgents: () => ({
    agents: [],
    fresh: true,
    refresh: mocks.refreshAgents,
  }),
}))

vi.mock("@/lib/api", () => ({
  acpDetectAgentLocalVersion: mocks.acpDetectAgentLocalVersion,
  acpListAgents: mocks.acpListAgents,
  acpPrepareNpxAgent: mocks.acpPrepareNpxAgent,
}))

function renderGate() {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      <StartupCodexGate>
        <div>Workspace</div>
      </StartupCodexGate>
    </NextIntlClientProvider>
  )
}

describe("StartupCodexGate", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.acpListAgents.mockResolvedValue([
      { agent_type: "codex", registry_version: "1.2.3" },
    ])
    mocks.acpDetectAgentLocalVersion.mockResolvedValue(null)
    mocks.refreshAgents.mockResolvedValue(undefined)
  })

  it("shows phase progress while checking and installing", async () => {
    let resolveAgents: (agents: unknown[]) => void = () => {}
    let resolveInstall: () => void = () => {}
    mocks.acpListAgents.mockReturnValue(
      new Promise<unknown[]>((resolve) => {
        resolveAgents = resolve
      })
    )
    mocks.acpPrepareNpxAgent.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveInstall = resolve
      })
    )

    renderGate()

    await waitFor(() => {
      expect(screen.getByRole("progressbar")).toHaveAttribute(
        "aria-valuenow",
        "30"
      )
    })

    await act(async () => {
      resolveAgents([{ agent_type: "codex", registry_version: "1.2.3" }])
      await Promise.resolve()
    })

    await waitFor(() => {
      expect(screen.getByRole("progressbar")).toHaveAttribute(
        "aria-valuenow",
        "75"
      )
    })

    await act(async () => {
      resolveInstall()
      await Promise.resolve()
    })
  })

  it("refreshes the shared agent list before opening the workspace", async () => {
    mocks.acpPrepareNpxAgent.mockResolvedValue(undefined)

    renderGate()

    await waitFor(() => expect(mocks.refreshAgents).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole("dialog")).toBeNull()
  })
})
