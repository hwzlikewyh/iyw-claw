"use client"

import { useCallback, useSyncExternalStore } from "react"
import { getServerBaseUrl } from "@/lib/transport"
import type { DbConversationSummary } from "@/lib/types"

const CHANGED_EVENT = "conversation-favorite-changed"
const STORAGE_PREFIX = "iyw-claw:conversation-favorite:v1:"

export function useConversationFavorite(summary: DbConversationSummary) {
  const key =
    STORAGE_PREFIX +
    JSON.stringify([
      getServerBaseUrl(),
      summary.agent_type,
      summary.id,
      summary.created_at,
    ])
  const subscribe = useCallback(
    (notify: () => void) => {
      const onStorage = (event: StorageEvent) => {
        if (event.key === key || event.key === null) notify()
      }
      const onChange = (event: Event) => {
        if ((event as CustomEvent<string>).detail === key) notify()
      }
      window.addEventListener("storage", onStorage)
      window.addEventListener(CHANGED_EVENT, onChange)
      return () => {
        window.removeEventListener("storage", onStorage)
        window.removeEventListener(CHANGED_EVENT, onChange)
      }
    },
    [key]
  )
  const read = useCallback(() => {
    try {
      return localStorage.getItem(key) === "true"
    } catch {
      return false
    }
  }, [key])
  const favorite = useSyncExternalStore(subscribe, read, () => false)
  const setFavorite = (value: boolean) => {
    if (value) localStorage.setItem(key, "true")
    else localStorage.removeItem(key)
    window.dispatchEvent(new CustomEvent(CHANGED_EVENT, { detail: key }))
  }
  return { favorite, setFavorite }
}
