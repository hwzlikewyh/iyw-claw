"use client"

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useSyncExternalStore,
} from "react"
import {
  CONVERSATION_DISPLAY_CHANGED_EVENT,
  CONVERSATION_DISPLAY_STORAGE_KEY,
  parseConversationDisplayPreferences,
  saveConversationDisplayPreferences,
  type ConversationDisplayMode,
  type ConversationDisplayPreferences,
} from "@/lib/conversation-display-preferences"

function subscribe(onStoreChange: () => void) {
  window.addEventListener("storage", onStoreChange)
  window.addEventListener(CONVERSATION_DISPLAY_CHANGED_EVENT, onStoreChange)
  return () => {
    window.removeEventListener("storage", onStoreChange)
    window.removeEventListener(
      CONVERSATION_DISPLAY_CHANGED_EVENT,
      onStoreChange
    )
  }
}

function getSnapshot() {
  try {
    return window.localStorage.getItem(CONVERSATION_DISPLAY_STORAGE_KEY) ?? ""
  } catch {
    return ""
  }
}

function getServerSnapshot() {
  return ""
}

interface ConversationDisplayContextValue extends ConversationDisplayPreferences {
  setMode: (mode: ConversationDisplayMode) => void
  setCollapseCompletedTurn: (value: boolean) => void
  setAutoOpenErrors: (value: boolean) => void
}

const ConversationDisplayContext =
  createContext<ConversationDisplayContextValue | null>(null)

export function ConversationDisplayProvider({
  children,
}: {
  children: React.ReactNode
}) {
  const rawPreferences = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot
  )
  const preferences = useMemo(
    () => parseConversationDisplayPreferences(rawPreferences || null),
    [rawPreferences]
  )

  const update = useCallback(
    (patch: Partial<ConversationDisplayPreferences>) => {
      const next = { ...preferences, ...patch }
      saveConversationDisplayPreferences(next)
      window.dispatchEvent(new Event(CONVERSATION_DISPLAY_CHANGED_EVENT))
    },
    [preferences]
  )

  const setMode = useCallback(
    (mode: ConversationDisplayMode) => update({ mode }),
    [update]
  )
  const setCollapseCompletedTurn = useCallback(
    (collapseCompletedTurn: boolean) => update({ collapseCompletedTurn }),
    [update]
  )
  const setAutoOpenErrors = useCallback(
    (autoOpenErrors: boolean) => update({ autoOpenErrors }),
    [update]
  )

  return (
    <ConversationDisplayContext.Provider
      value={{
        ...preferences,
        setMode,
        setCollapseCompletedTurn,
        setAutoOpenErrors,
      }}
    >
      {children}
    </ConversationDisplayContext.Provider>
  )
}

export function useConversationDisplayPreferences() {
  const context = useContext(ConversationDisplayContext)
  if (!context) {
    throw new Error(
      "useConversationDisplayPreferences must be used within ConversationDisplayProvider"
    )
  }
  return context
}
