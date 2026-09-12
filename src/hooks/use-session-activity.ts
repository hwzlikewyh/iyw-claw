"use client"

import { useCallback, useSyncExternalStore } from "react"
import { useOptionalConnectionStore } from "@/contexts/acp-connections-context"

export function useSessionActivity(contextKey?: string) {
  const store = useOptionalConnectionStore()
  const subscribe = useCallback(
    (listener: () => void) =>
      contextKey && store ? store.subscribeKey(contextKey, listener) : () => {},
    [store, contextKey]
  )
  const snapshot = useCallback(
    () =>
      contextKey ? (store?.getConnection(contextKey)?.activity ?? null) : null,
    [store, contextKey]
  )
  return useSyncExternalStore(subscribe, snapshot, () => null)
}
