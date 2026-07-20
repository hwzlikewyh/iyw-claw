import "@testing-library/jest-dom/vitest"

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { UserMemorySettings } from "./user-memory-settings"

const { getUserMemorySettings, updateUserMemorySettings } = vi.hoisted(() => ({
  getUserMemorySettings: vi.fn(),
  updateUserMemorySettings: vi.fn(),
}))

vi.mock("next-intl", () => ({
  useTranslations:
    () => (key: string, values?: Record<string, string | number>) =>
      values ? `${key}:${JSON.stringify(values)}` : key,
}))

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}))

vi.mock("@/lib/api", () => ({
  getUserMemorySettings,
  updateUserMemorySettings,
}))

const settingsSnapshot = {
  enabled: true,
  agentWriteEnabled: true,
  inheritToSubagents: true,
  perAgent: {
    claude_code: true,
    codex: true,
    open_code: true,
    gemini: true,
    open_claw: true,
    cline: true,
    hermes: true,
    code_buddy: true,
    kimi_code: true,
    pi: true,
    grok: true,
  },
  documents: {
    memory: {
      id: "memory",
      fileName: "user-memory.md",
      path: "D:\\iyw-data\\user-memory.md",
      content: "Durable preference",
      etag: "memory-etag",
      enabled: true,
      readonly: false,
    },
    profile: {
      id: "profile",
      fileName: "user-profile.md",
      path: "D:\\iyw-data\\user-profile.md",
      content: "User profile",
      etag: "profile-etag",
      enabled: true,
      readonly: false,
    },
    soul: {
      id: "soul",
      fileName: "user-soul.md",
      path: "D:\\iyw-data\\user-soul.md",
      content: "User values",
      etag: "soul-etag",
      enabled: true,
      readonly: false,
    },
  },
  revision: "revision-1",
  staleRunningSessions: 0,
}

describe("UserMemorySettings", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getUserMemorySettings.mockResolvedValue(settingsSnapshot)
    updateUserMemorySettings.mockResolvedValue({
      settings: {
        ...settingsSnapshot,
        documents: {
          ...settingsSnapshot.documents,
          memory: {
            ...settingsSnapshot.documents.memory,
            content: "Updated preference",
            etag: "memory-etag-2",
          },
        },
        revision: "revision-2",
        staleRunningSessions: 2,
      },
      affectedRunningSessions: 2,
    })
  })

  it("loads document content and resolved paths from the settings snapshot", async () => {
    render(<UserMemorySettings />)

    expect(await screen.findByDisplayValue("Durable preference")).toBeVisible()
    expect(screen.getByText(/D:\\iyw-data\\user-memory\.md/)).toBeVisible()
  })

  it("shows new-conversation state even when memory is globally disabled", async () => {
    getUserMemorySettings.mockResolvedValueOnce({
      ...settingsSnapshot,
      enabled: false,
      staleRunningSessions: 2,
    })

    render(<UserMemorySettings />)

    expect(
      await screen.findByText('status.newConversationRequired:{"count":2}')
    ).toBeVisible()
    expect(screen.getByText("status.disabled")).toBeVisible()
  })

  it("does not expose an editor when the initial settings load fails", async () => {
    getUserMemorySettings.mockRejectedValueOnce(
      new Error("User memory backend unavailable")
    )

    render(<UserMemorySettings />)

    const alert = await screen.findByRole("alert")
    expect(alert).toHaveTextContent("User memory backend unavailable")
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument()
    expect(screen.getByRole("button", { name: "reload" })).toBeVisible()
  })

  it("saves a changed document with revision and etag guards", async () => {
    render(<UserMemorySettings />)

    const editor = await screen.findByDisplayValue("Durable preference")
    fireEvent.change(editor, { target: { value: "Updated preference" } })
    fireEvent.click(screen.getByRole("button", { name: "save" }))

    await waitFor(() =>
      expect(updateUserMemorySettings).toHaveBeenCalledWith({
        expectedRevision: "revision-1",
        documents: {
          memory: {
            content: "Updated preference",
            expectedEtag: "memory-etag",
          },
        },
      })
    )
    expect(
      await screen.findByText('status.newConversationRequired:{"count":2}')
    ).toBeVisible()
  })

  it("preserves unsaved drafts while switching documents", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false)
    render(<UserMemorySettings />)

    const memoryEditor = await screen.findByDisplayValue("Durable preference")
    fireEvent.change(memoryEditor, {
      target: { value: "Unsaved memory draft" },
    })
    fireEvent.click(
      screen.getByRole("button", { name: /documents\.profile\.label/ })
    )

    expect(await screen.findByDisplayValue("User profile")).toBeVisible()
    fireEvent.click(
      screen.getByRole("button", { name: /documents\.memory\.label/ })
    )
    expect(screen.getByDisplayValue("Unsaved memory draft")).toBeVisible()
    expect(confirm).not.toHaveBeenCalled()
  })

  it("saves only changed policy, document, and agent switches", async () => {
    updateUserMemorySettings.mockResolvedValueOnce({
      settings: {
        ...settingsSnapshot,
        enabled: false,
        agentWriteEnabled: false,
        inheritToSubagents: false,
        perAgent: { ...settingsSnapshot.perAgent, codex: false },
        documents: {
          ...settingsSnapshot.documents,
          profile: {
            ...settingsSnapshot.documents.profile,
            enabled: false,
          },
        },
        revision: "revision-2",
        staleRunningSessions: 1,
      },
      affectedRunningSessions: 1,
    })
    render(<UserMemorySettings />)

    await screen.findByDisplayValue("Durable preference")
    fireEvent.click(
      screen.getByRole("switch", { name: "policy.agentWriteEnabled" })
    )
    fireEvent.click(
      screen.getByRole("switch", { name: "policy.inheritToSubagents" })
    )
    fireEvent.click(
      screen.getByRole("switch", {
        name: /policy\.documentToggle.*documents\.profile\.label/,
      })
    )
    fireEvent.click(
      screen.getByRole("switch", { name: /policy\.agentToggle.*星河/ })
    )
    fireEvent.click(screen.getByRole("switch", { name: "policy.enabled" }))
    fireEvent.click(screen.getByRole("button", { name: "save" }))

    await waitFor(() =>
      expect(updateUserMemorySettings).toHaveBeenCalledWith({
        expectedRevision: "revision-1",
        enabled: false,
        agentWriteEnabled: false,
        inheritToSubagents: false,
        perAgent: { codex: false },
        documents: {
          profile: { enabled: false },
        },
      })
    )
  })

  it("keeps the local draft when a concurrent update causes a conflict", async () => {
    updateUserMemorySettings.mockRejectedValueOnce({
      code: "conflict",
      message: "User memory settings changed; reload before saving",
    })
    render(<UserMemorySettings />)

    const editor = await screen.findByDisplayValue("Durable preference")
    fireEvent.change(editor, { target: { value: "Keep this local draft" } })
    fireEvent.click(screen.getByRole("button", { name: "save" }))

    expect(await screen.findByRole("alert")).toHaveTextContent("saveConflict")
    expect(screen.getByDisplayValue("Keep this local draft")).toBeVisible()
  })

  it("does not reload over an unsaved draft without confirmation", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false)
    render(<UserMemorySettings />)

    const editor = await screen.findByDisplayValue("Durable preference")
    fireEvent.change(editor, { target: { value: "Keep before reload" } })
    fireEvent.click(screen.getByRole("button", { name: "reload" }))

    await waitFor(() => expect(confirm).toHaveBeenCalledOnce())
    expect(getUserMemorySettings).toHaveBeenCalledOnce()
    expect(screen.getByDisplayValue("Keep before reload")).toBeVisible()
  })
})
