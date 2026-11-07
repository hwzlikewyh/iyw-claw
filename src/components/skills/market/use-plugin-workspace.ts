"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { prepareConnectorConversation } from "@/lib/conversation-connectors"
import { getTransport } from "@/lib/transport"
import { onNewConversationOpened } from "@/stores/tab-store"
import type { TabItem } from "@/contexts/tab-context"

export function usePluginWorkspace(tabId: string | null) {
  const t = useTranslations("PluginPreparation")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const alive = useRef(true)
  const running = useRef(false)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  const bind = async () => {
    if (!tabId || running.current) return null
    const original = useTabStore
      .getState()
      .rawTabs.find((tab) => tab.id === tabId)
    if (!original) return null
    const transport = getTransport()
    let canceled = false
    const stop = onNewConversationOpened(() => {
      canceled = true
    })
    const unsubscribe = useTabStore.subscribe((store) => {
      const current = store.rawTabs.find((tab) => tab.id === tabId)
      if (
        store.activeTabId !== tabId ||
        !current ||
        store.draftRetargetRequests.some(
          (request) => request.tabId === tabId
        ) ||
        (current.conversationId == null &&
          (current.folderId !== original.folderId ||
            current.agentType !== original.agentType ||
            current.isChat !== original.isChat))
      )
        canceled = true
    })
    running.current = true
    setBusy(true)
    setError("")
    try {
      const prepared = await prepareConnectorConversation(
        tabId,
        t("newConversation")
      )
      if (!alive.current || canceled || getTransport() !== transport)
        return null
      return bindPreparedContext(original, prepared)
    } catch (cause) {
      if (alive.current) setError(String(cause))
      return null
    } finally {
      stop()
      unsubscribe()
      running.current = false
      if (alive.current) setBusy(false)
    }
  }
  return { busy, error, bind }
}

function bindPreparedContext(
  original: TabItem,
  prepared: Awaited<ReturnType<typeof prepareConnectorConversation>>
) {
  const current = useTabStore
    .getState()
    .rawTabs.find((tab) => tab.id === original.id)
  if (!current || current.agentType !== original.agentType) return null
  if (current.conversationId != null) return current.conversationId
  if (
    current.folderId !== original.folderId ||
    current.isChat !== original.isChat ||
    current.workingDir !== original.workingDir
  )
    return null
  prepared.bind()
  return (
    useTabStore.getState().rawTabs.find((tab) => tab.id === original.id)
      ?.conversationId ?? null
  )
}
