"use client"

import { useTabStore } from "@/contexts/tab-context"
import { usePluginPanelChoices } from "./use-plugin-panel-choices"
import { usePluginPanelTabs } from "./use-plugin-panel-tabs"
import { usePluginPanelUpdates } from "./use-plugin-panel-updates"
import { useInstalledPluginOpen } from "./use-installed-plugin-open"
import { useConversationPanelTransfer } from "./use-conversation-panel-transfer"
import { notifySkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"
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
  useConversationPanelTransfer(tabs, conversationId)
  const catalog = usePluginPanelChoices(conversationId, tabs.transport)
  const updates = usePluginPanelUpdates(tabs.tabs, tabs.transport)
  const selection = useInstalledPluginOpen(tabs)
  return {
    ...tabs,
    ...catalog,
    refresh: notifySkillMarketInstallationsChanged,
    updates,
    conversationId,
    open: selection.open,
    busy: tabs.busy || selection.busy,
    error: selection.error || tabs.error || catalog.catalogError,
  }
}
