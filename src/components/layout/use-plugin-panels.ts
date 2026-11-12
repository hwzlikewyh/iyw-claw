"use client"

import { useTabStore } from "@/contexts/tab-context"
import { usePluginPanelChoices } from "./use-plugin-panel-choices"
import { usePluginPanelTabs } from "./use-plugin-panel-tabs"
import { usePluginPanelUpdates } from "./use-plugin-panel-updates"
export type { PluginPanelTab } from "./use-plugin-panel-tabs"

export function usePluginPanels() {
  const conversationId = useTabStore(
    (store) =>
      store.tabs.find((tab) => tab.id === store.activeTabId)?.conversationId ??
      null
  )
  const conversations = useTabStore((store) =>
    [
      ...new Set(
        store.tabs.flatMap((tab) =>
          tab.conversationId ? [tab.conversationId] : []
        )
      ),
    ]
      .sort()
      .join(",")
  )
  const tabs = usePluginPanelTabs(conversationId, conversations)
  const catalog = usePluginPanelChoices(conversationId, tabs.transport)
  const updates = usePluginPanelUpdates(tabs.tabs, tabs.transport)
  return {
    ...tabs,
    ...catalog,
    updates,
    conversationId,
    error: tabs.error || catalog.catalogError,
  }
}
