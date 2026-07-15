import { describe, expect, it } from "vitest"
import {
  OPEN_SETTINGS_DIALOG_EVENT,
  buildSettingsPath,
  normalizeSettingsSection,
  requestSettingsDialog,
  type OpenSettingsDialogDetail,
} from "./settings-navigation"

describe("settings navigation", () => {
  it("reports that the in-window settings dialog handled the request", () => {
    let received: OpenSettingsDialogDetail | null = null
    const listener = (event: Event) => {
      const customEvent = event as CustomEvent<OpenSettingsDialogDetail>
      customEvent.preventDefault()
      received = customEvent.detail
    }

    window.addEventListener(OPEN_SETTINGS_DIALOG_EVENT, listener)
    try {
      expect(requestSettingsDialog("agents", { agentType: "codex" })).toBe(true)
      expect(received).toEqual({ section: "agents", agentType: "codex" })
    } finally {
      window.removeEventListener(OPEN_SETTINGS_DIALOG_EVENT, listener)
    }
  })

  it("falls back when no settings dialog host is mounted", () => {
    expect(requestSettingsDialog("appearance")).toBe(false)
  })

  it("builds the current-window settings path with an agent query", () => {
    expect(buildSettingsPath("agents", { agentType: "claude_code" })).toBe(
      "/settings/agents?agent=claude_code"
    )
  })

  it("builds paths for usage and user memory settings", () => {
    expect(buildSettingsPath("usage")).toBe("/settings/usage")
    expect(buildSettingsPath("user-memory")).toBe("/settings/user-memory")
    expect(buildSettingsPath("logs")).toBe("/settings/logs")
    expect(normalizeSettingsSection("user-memory")).toBe("user-memory")
    expect(buildSettingsPath("internet-tools")).toBe("/settings/internet-tools")
    expect(normalizeSettingsSection("internet-tools")).toBe("internet-tools")
  })

  it("treats the removed web service settings section as the default page", () => {
    expect(normalizeSettingsSection("web-service")).toBe("appearance")
    expect(buildSettingsPath("web-service")).toBe("/settings/appearance")
  })
})
