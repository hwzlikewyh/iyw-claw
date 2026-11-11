"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { prepareConnectorConversation } from "@/lib/conversation-connectors"
import { getTransport } from "@/lib/transport"
import { onNewConversationOpened } from "@/stores/tab-store"
import type { TabItem } from "@/contexts/tab-context"
import { preparePluginDraft } from "@/lib/plugin-composer-context"

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
    const scope = watchPreparedWorkspace(original)
    running.current = true
    setBusy(true)
    setError("")
    try {
      const prepared = await prepareConnectorConversation(
        tabId,
        t("newConversation")
      )
      if (!alive.current || !scope.current()) return null
      return bindPreparedContext(original, prepared)
    } catch (cause) {
      if (alive.current && scope.current()) setError(String(cause))
      return null
    } finally {
      scope.dispose()
      running.current = false
      if (alive.current) setBusy(false)
    }
  }
  return { busy, error, bind }
}

function watchPreparedWorkspace(original: TabItem) {
  const transport = getTransport()
  let canceled = false
  const stop = onNewConversationOpened(() => {
    canceled = true
  })
  const unsubscribe = useTabStore.subscribe((store) => {
    const current = store.rawTabs.find((tab) => tab.id === original.id)
    if (
      store.activeTabId !== original.id ||
      !current ||
      store.draftRetargetRequests.some(
        (request) => request.tabId === original.id
      ) ||
      (current.conversationId == null &&
        (current.folderId !== original.folderId ||
          current.agentType !== original.agentType ||
          current.isChat !== original.isChat ||
          current.workingDir !== original.workingDir))
    )
      canceled = true
  })
  return {
    current: () => !canceled && getTransport() === transport,
    dispose: () => {
      stop()
      unsubscribe()
    },
  }
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
  preparePluginDraft(original.id, prepared.id)
  prepared.bind()
  return (
    useTabStore.getState().rawTabs.find((tab) => tab.id === original.id)
      ?.conversationId ?? null
  )
}
