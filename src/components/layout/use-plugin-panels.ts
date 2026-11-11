"use client"

import { useEffect } from "react"
import {
  clearPluginComposerContext,
  usePluginComposerContext,
} from "@/lib/plugin-composer-context"
import { usePluginPanelRegistry } from "@/stores/plugin-panel-registry"
import { useTabStore } from "@/contexts/tab-context"
import { usePluginPanelChoices } from "./use-plugin-panel-choices"
import { usePluginPanelTabs } from "./use-plugin-panel-tabs"
import { usePluginPanelUpdates } from "./use-plugin-panel-updates"
import { usePluginPanelOpenIntent } from "./use-plugin-panel-open-intent"
export type { PluginPanelTab } from "./use-plugin-panel-tabs"

export function usePluginPanels() {
  useEffect(() => {
    const tabs = useTabStore.subscribe((state) => {
      for (const id of Object.keys(usePluginComposerContext.getState().entries))
        if (!state.rawTabs.some((tab) => tab.id === id))
          clearPluginComposerContext(id)
    })
    return () => {
      tabs()
      clearPluginComposerContext()
      usePluginPanelRegistry.getState().clear()
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
  const intent = usePluginPanelOpenIntent({
    conversationId,
    transport: tabs.transport,
    open: tabs.open,
  })
  return {
    ...tabs,
    ...catalog,
    updates,
    conversationId,
    open: intent.open,
    busy: tabs.busy || intent.busy,
    error: intent.error || tabs.error || catalog.catalogError,
  }
}
