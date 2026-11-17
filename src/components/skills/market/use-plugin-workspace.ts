"use client"

import { useTabStore } from "@/contexts/tab-context"
import { useState, useSyncExternalStore } from "react"
import { toErrorMessage } from "@/lib/app-error"
import {
  draftPluginContext,
  onDraftPluginContextChanged,
  prepareDraftPluginContext,
} from "@/lib/plugin-draft-context"

// 未发送的草稿只准备插件临时工作区，不创建会话记录。
export function usePluginWorkspace(tabId: string | null) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const contextId = useSyncExternalStore(
    onDraftPluginContextChanged,
    () => (tabId ? (draftPluginContext(tabId)?.id ?? null) : null),
    () => null
  )
  const bind = async () => {
    if (!tabId || busy) return null
    const tab = useTabStore.getState().tabs.find((item) => item.id === tabId)
    if (!tab) return null
    if (tab.conversationId != null) return tab.conversationId
    setBusy(true)
    setError("")
    try {
      return await prepareDraftPluginContext(tabId)
    } catch (cause) {
      setError(toErrorMessage(cause))
      return null
    } finally {
      setBusy(false)
    }
  }
  return { busy, error, bind, contextId }
}
