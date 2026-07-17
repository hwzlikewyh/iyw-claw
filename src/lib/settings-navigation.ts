import type { AgentType } from "./types"

export const OPEN_SETTINGS_DIALOG_EVENT = "app://open-settings-dialog"

export type SettingsSection =
  | "appearance"
  | "general"
  | "usage"
  | "user-memory"
  | "agents"
  | "mcp"
  | "experts"
  | "office-tools"
  | "internet-tools"
  | "quick-messages"
  | "shortcuts"
  | "version-control"
  | "chat-channels"
  | "system"
  | "skills"
  | "model-providers"
  | "logs"

export interface OpenSettingsDialogOptions {
  agentType?: AgentType | null
}

export interface OpenSettingsDialogDetail {
  section: SettingsSection
  agentType: AgentType | null
}

const DEFAULT_SETTINGS_SECTION: SettingsSection = "appearance"

export function normalizeSettingsSection(
  section?: string | null
): SettingsSection {
  switch (section) {
    case "appearance":
    case "general":
    case "usage":
    case "user-memory":
    case "agents":
    case "mcp":
    case "experts":
    case "office-tools":
    case "internet-tools":
    case "quick-messages":
    case "shortcuts":
    case "version-control":
    case "chat-channels":
    case "system":
    case "skills":
    case "model-providers":
    case "logs":
      return section
    default:
      return DEFAULT_SETTINGS_SECTION
  }
}

export function settingsSectionToPath(section?: string | null): string {
  return `/settings/${normalizeSettingsSection(section)}`
}

export function settingsSectionToNavPath(section?: string | null): string {
  const normalized = normalizeSettingsSection(section)
  switch (normalized) {
    case "experts":
    case "office-tools":
    case "internet-tools":
      return "/settings/skills"
    default:
      return settingsSectionToPath(normalized)
  }
}

export function settingsPathToSection(path: string): SettingsSection {
  const pathname = path.split("?")[0]?.replace(/\/index\.html$/, "") ?? ""
  const match = pathname.match(/\/settings\/([^/]+)$/)
  return normalizeSettingsSection(match?.[1])
}

export function buildSettingsPath(
  section?: string | null,
  options?: OpenSettingsDialogOptions
): string {
  const path = settingsSectionToPath(section)
  if (normalizeSettingsSection(section) !== "agents" || !options?.agentType) {
    return path
  }

  const params = new URLSearchParams({ agent: options.agentType })
  return `${path}?${params.toString()}`
}

export function requestSettingsDialog(
  section?: string | null,
  options?: OpenSettingsDialogOptions
): boolean {
  if (typeof window === "undefined") return false

  const event = new CustomEvent<OpenSettingsDialogDetail>(
    OPEN_SETTINGS_DIALOG_EVENT,
    {
      cancelable: true,
      detail: {
        section: normalizeSettingsSection(section),
        agentType: options?.agentType ?? null,
      },
    }
  )

  return !window.dispatchEvent(event)
}
