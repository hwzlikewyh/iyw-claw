"use client"

import { useEffect, useRef } from "react"
import {
  watchDraftPluginContexts,
  releaseDraftPluginContexts,
} from "@/lib/plugin-draft-context"
import { useTabStore } from "@/contexts/tab-context"
import { usePluginPanelChoices } from "./use-plugin-panel-choices"
import { usePluginPanelTabs } from "./use-plugin-panel-tabs"
import { usePluginPanelUpdates } from "./use-plugin-panel-updates"
import { useInstalledPluginOpen } from "./use-installed-plugin-open"
import { usePluginPanelRetry } from "./use-plugin-panel-retry"
import { toast } from "sonner"
import { useConversationPanelTransfer } from "./use-conversation-panel-transfer"
import { notifySkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"
export type { PluginPanelTab } from "./use-plugin-panel-tabs"

export function usePluginPanels() {
  const tabId = useTabStore((store) => store.activeTabId)
  const generation = useRef({ value: 0 })
  useEffect(() => {
    const lifecycle = generation.current
    const current = ++lifecycle.value
    const stop = watchDraftPluginContexts()
    return () => {
      stop()
      queueMicrotask(() => {
        if (lifecycle.value === current) releaseDraftPluginContexts()
      })
    }
  }, [])
  const conversationId = useTabStore(
    (store) =>
      store.tabs.find((tab) => tab.id === store.activeTabId)?.conversationId ??
      null
  )
  const conversations = useTabStore((store) =>
    [
      ...new Set(
        store.tabs.flatMap((tab) => [
          tab.id,
          ...(tab.conversationId ? [String(tab.conversationId)] : []),
        ])
      ),
    ]
      .sort()
      .join(",")
  )
  const tabs = usePluginPanelTabs(conversationId, conversations)
  useConversationPanelTransfer(tabs, conversationId)
  const catalog = usePluginPanelChoices(conversationId, tabs.transport)
  const updates = usePluginPanelUpdates(tabs.tabs, tabs.transport)
  const selection = useInstalledPluginOpen(tabs)
  const retry = usePluginPanelRetry({
    tabs: tabs.tabs,
    close: tabs.close,
    transport: tabs.transport,
    onError: (message) => toast.error(message),
  })
  return {
    ...tabs,
    ...catalog,
    refresh: notifySkillMarketInstallationsChanged,
    updates,
    conversationId,
    tabId,
    open: selection.open,
    retry,
    openingApp: tabs.openingApp ?? selection.openingApp,
    busy: tabs.busy || selection.busy,
    error: selection.error || tabs.error || catalog.catalogError,
  }
}
