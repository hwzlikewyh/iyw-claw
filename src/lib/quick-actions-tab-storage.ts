"use client"

const QUICK_ACTIONS_TAB_KEY = "workspace:quick-actions-tab"

/** Which skill group the welcome-page quick actions show. */
export type QuickActionsTab = "common" | "research"

/**
 * Last-picked quick-actions tab, restored when a new conversation opens.
 * Defaults to "common"; an absent, legacy, or polluted value falls back to
 * polluted value falls back to that default.
 */
export function loadQuickActionsTab(): QuickActionsTab {
  if (typeof window === "undefined") return "common"
  try {
    const raw = localStorage.getItem(QUICK_ACTIONS_TAB_KEY)
    if (raw === "common" || raw === "research") return raw
  } catch {
    /* ignore */
  }
  return "common"
}

export function saveQuickActionsTab(value: QuickActionsTab): void {
  if (typeof window === "undefined") return
  try {
    localStorage.setItem(QUICK_ACTIONS_TAB_KEY, value)
  } catch {
    /* ignore */
  }
}
