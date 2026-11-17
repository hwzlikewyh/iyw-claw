import { useEffect, useRef } from "react"
import {
  useAuxPanelContext,
  type AuxPanelTab,
} from "@/contexts/aux-panel-context"
import type { PluginPanelTab } from "./use-plugin-panel-tabs"
import { useTabStore } from "@/stores/tab-store"
import { draftPluginTab } from "@/lib/plugin-draft-context"

type Selection = { activeTab: AuxPanelTab; isOpen: boolean }

// 仅记忆插件面板；文件和产物入口继续沿用辅助面板原有行为。
export function useConversationPluginPanel(
  tabId: string | null,
  tabs: PluginPanelTab[]
) {
  const { activeTab, isOpen, setActiveTab, setOpen } = useAuxPanelContext()
  const saved = useRef(new Map<string, Selection>())
  const previous = useRef({ tabId, activeTab, isOpen })
  useEffect(() => {
    const last = previous.current
    const selections = saved.current
    for (const [id, selection] of selections)
      if (!ownsSelection(tabs, id, selection)) selections.delete(id)
    if (last.tabId === tabId) {
      previous.current = { tabId, activeTab, isOpen }
      return
    }
    if (last.tabId !== null) {
      if (ownsSelection(tabs, last.tabId, last))
        selections.set(last.tabId, last)
      else selections.delete(last.tabId)
    }
    const next = tabId === null ? undefined : selections.get(tabId)
    const target =
      next ??
      (last.activeTab.startsWith("app:")
        ? { activeTab: "home" as const, isOpen: false }
        : { activeTab, isOpen })
    previous.current = { tabId, ...target }
    if (activeTab !== target.activeTab) setActiveTab(target.activeTab)
    if (isOpen !== target.isOpen) setOpen(target.isOpen)
  }, [tabId, tabs, activeTab, isOpen, setActiveTab, setOpen])
}

function ownsSelection(
  tabs: PluginPanelTab[],
  id: string,
  selection: Selection
) {
  const conversation = useTabStore.getState().tabs.find((tab) => tab.id === id)
  if (!conversation) return false
  return tabs.some(
    (tab) =>
      `app:${tab.id}` === selection.activeTab &&
      tab.conversationId !== null &&
      (tab.conversationId === conversation.conversationId ||
        (tab.conversationId < 0 && draftPluginTab(tab.conversationId) === id))
  )
}
