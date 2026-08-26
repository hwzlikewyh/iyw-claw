"use client"

export type ConversationDisplayMode = "summary" | "full" | "minimal"

export interface ConversationDisplayPreferences {
  mode: ConversationDisplayMode
  collapseCompletedTurn: boolean
  autoOpenErrors: boolean
}

export const DEFAULT_CONVERSATION_DISPLAY_PREFERENCES: ConversationDisplayPreferences =
  {
    mode: "summary",
    collapseCompletedTurn: true,
    autoOpenErrors: true,
  }

export const CONVERSATION_DISPLAY_STORAGE_KEY =
  "iyw-claw-conversation-display-preferences"

export const CONVERSATION_DISPLAY_CHANGED_EVENT =
  "iyw-claw-conversation-display-preferences-changed"

function isDisplayMode(value: unknown): value is ConversationDisplayMode {
  return value === "summary" || value === "full" || value === "minimal"
}

export function parseConversationDisplayPreferences(
  raw: string | null
): ConversationDisplayPreferences {
  try {
    if (!raw) return DEFAULT_CONVERSATION_DISPLAY_PREFERENCES
    const parsed: unknown = JSON.parse(raw)
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return DEFAULT_CONVERSATION_DISPLAY_PREFERENCES
    }

    const value = parsed as Record<string, unknown>
    return {
      mode: isDisplayMode(value.mode)
        ? value.mode
        : DEFAULT_CONVERSATION_DISPLAY_PREFERENCES.mode,
      collapseCompletedTurn:
        typeof value.collapseCompletedTurn === "boolean"
          ? value.collapseCompletedTurn
          : DEFAULT_CONVERSATION_DISPLAY_PREFERENCES.collapseCompletedTurn,
      autoOpenErrors:
        typeof value.autoOpenErrors === "boolean"
          ? value.autoOpenErrors
          : DEFAULT_CONVERSATION_DISPLAY_PREFERENCES.autoOpenErrors,
    }
  } catch {
    return DEFAULT_CONVERSATION_DISPLAY_PREFERENCES
  }
}

export function loadConversationDisplayPreferences(): ConversationDisplayPreferences {
  if (typeof window === "undefined") {
    return DEFAULT_CONVERSATION_DISPLAY_PREFERENCES
  }

  try {
    return parseConversationDisplayPreferences(
      localStorage.getItem(CONVERSATION_DISPLAY_STORAGE_KEY)
    )
  } catch {
    return DEFAULT_CONVERSATION_DISPLAY_PREFERENCES
  }
}

export function saveConversationDisplayPreferences(
  preferences: ConversationDisplayPreferences
): void {
  if (typeof window === "undefined") return
  try {
    localStorage.setItem(
      CONVERSATION_DISPLAY_STORAGE_KEY,
      JSON.stringify(preferences)
    )
  } catch {
    // Storage may be unavailable in private or embedded webviews.
  }
}
