"use client"

import { useEffect, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { useBrowserSessionBridge } from "@/hooks/use-browser-session-bridge"
import { isConversationWindowPending } from "@/lib/conversation-window-state"
import { ConversationWindowGate } from "@/components/workspace/conversation-window-gate"
import { useShallow } from "zustand/react/shallow"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useAcpActions } from "@/contexts/acp-connections-context"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import { useSortedAvailableAgents } from "@/hooks/use-sorted-available-agents"
import { onTransportReconnect, subscribe } from "@/lib/platform"
import {
  runCorrectionOnce,
  runRecoveryOnce,
  useTabStore,
  type TabItem,
} from "@/stores/tab-store"
import {
  CONVERSATION_CHANGED_EVENT,
  type ConversationChange,
} from "@/lib/types"

export type { TabItem }
export { useTabStore, useTabActions } from "@/stores/tab-store"

interface TabProviderProps {
  children: ReactNode
}

/**
 * Thin lifecycle glue for `useTabStore`: injects the React-land dependencies
 * (i18n labels, `activateConversationPane`, `acpDisconnect`, agent availability)
 * and drives the effects that need a React lifecycle — session initialization,
 * sub-session `conversation://changed` subscriptions, the provisional-agent
 * correction gate, and the initial draft. All state and logic live in the
 * store; this component renders nothing but `children`.
 */
export function TabProvider({ children }: TabProviderProps) {
  useBrowserSessionBridge()
  const t = useTranslations("Folder.tabContext")
  const { activateConversationPane } = useWorkspaceActions()
  const {
    disconnect: acpDisconnect,
    disconnectForReplacement: acpDisconnectForReplacement,
  } = useAcpActions()
  const { sortedTypes: sortedAvailableAgents, fresh: agentsFresh } =
    useSortedAvailableAgents()

  // App-workspace gates for the correction / recovery / child-reconcile effects.
  const foldersHydrated = useAppWorkspaceStore((s) => s.foldersHydrated)
  const conversations = useAppWorkspaceStore((s) => s.conversations)
  const conversationsLoading = useAppWorkspaceStore(
    (s) => s.conversationsLoading
  )

  // Tab-store slices used only as effect dependencies (this component renders
  // nothing, so subscribing here doesn't cascade to any consumer).
  const rawTabs = useTabStore((s) => s.rawTabs)
  const activeTabId = useTabStore((s) => s.activeTabId)
  const previewReplacedTabIds = useTabStore((s) => s.previewReplacedTabIds)
  const draftRetargetRequests = useTabStore((s) => s.draftRetargetRequests)
  const tabsHydrated = useTabStore((s) => s.tabsHydrated)
  const reseedTick = useTabStore((s) => s.reseedTick)

  // ── Runtime dependency injection ─────────────────────────────────────────────
  // Labels first (declared before hydrate) so seed titles are translated before
  // the hydration effect runs.
  useEffect(() => {
    useTabStore.getState().setLabels({
      loadingConversation: t("loadingConversation"),
      newConversation: t("newConversation"),
      untitledConversation: t("untitledConversation"),
    })
  }, [t])

  useEffect(() => {
    useTabStore.getState().setSideEffects({
      activateConversationPane,
      acpDisconnect,
      acpDisconnectForReplacement,
    })
  }, [activateConversationPane, acpDisconnect, acpDisconnectForReplacement])

  useEffect(() => {
    useTabStore
      .getState()
      .setAgentAvailability(sortedAvailableAgents, agentsFresh)
  }, [sortedAvailableAgents, agentsFresh])

  // Sync the active tab's folderId up to the app-workspace store so derived
  // consumers (useActiveFolder, branch polling) reflect the focused folder.
  useEffect(() => {
    useTabStore.getState().syncActiveFolderId()
  }, [rawTabs, activeTabId])

  // Fire the preview-replacement callbacks and trim the consumed queue.
  useEffect(() => {
    useTabStore.getState().consumePreviewReplaced()
  }, [previewReplacedTabIds])

  // Disconnect + retarget each queued draft-retarget request.
  useEffect(() => {
    useTabStore.getState().consumeDraftRetargets()
  }, [draftRetargetRequests])

  // 页签仅保留在本次运行中，启动时不读取上次打开的会话。
  useEffect(() => useTabStore.getState().hydrate(), [])

  // Reconcile the sub-session summary cache to the open child tabs.
  useEffect(() => {
    useTabStore.getState().reconcileChildSummaries()
  }, [rawTabs, conversations, conversationsLoading, reseedTick])

  // Keep seeded sub-session summaries live off the global conversation channel.
  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void (async () => {
      const dispose = await subscribe<ConversationChange>(
        CONVERSATION_CHANGED_EVENT,
        (change) => useTabStore.getState().handleChildConversationChange(change)
      )
      if (disposed) dispose()
      else unlisten = dispose
    })()
    const offReconnect = onTransportReconnect(() =>
      useTabStore.getState().handleChildReconnect()
    )
    return () => {
      disposed = true
      unlisten?.()
      offReconnect?.()
    }
  }, [])

  // Correction must wait for the fresh agent list, hydrated tabs, and hydrated
  // folders (so folder defaults resolve). One-shot per session via the store.
  useEffect(() => {
    if (!agentsFresh) return
    if (!tabsHydrated) return
    if (!foldersHydrated) return
    runCorrectionOnce()
  }, [agentsFresh, tabsHydrated, foldersHydrated])

  // 初始化后保留一个新对话；手动打开的会话不受影响。
  useEffect(() => {
    if (!tabsHydrated || !foldersHydrated) return
    if (isConversationWindowPending()) return
    if (rawTabs.length > 0) return
    runRecoveryOnce()
  }, [tabsHydrated, foldersHydrated, rawTabs])

  return <ConversationWindowGate>{children}</ConversationWindowGate>
}

export interface TabContextValue {
  tabs: TabItem[]
  activeTabId: string | null
  tabsHydrated: boolean
  isTileMode: boolean
  openTab: (
    folderId: number,
    conversationId: number,
    agentType: TabItem["agentType"],
    pin?: boolean,
    title?: string
  ) => void
  closeTab: (tabId: string) => void
  closeConversationTab: (
    folderId: number,
    conversationId: number,
    agentType: TabItem["agentType"]
  ) => void
  closeOtherTabs: (tabId: string) => void
  closeAllTabs: () => void
  closeTabsByFolder: (folderId: number) => void
  switchTab: (tabId: string) => void
  pinTab: (tabId: string) => void
  toggleTileMode: () => void
  consumeRemoteActivation: () => boolean
  openNewConversationTab: (
    folderId: number,
    workingDir: string,
    options?: {
      inheritFromActive?: boolean
      folderDefaultAgent?: TabItem["agentType"] | null
      initialComposerText?: string
    }
  ) => void
  openChatModeTab: (options?: { initialComposerText?: string }) => void
  setChatDraftWorkingDir: (tabId: string, workingDir: string) => void
  confirmDraftAgent: (tabId: string, agentType: TabItem["agentType"]) => void
  setDraftAgentFromFallback: (
    tabId: string,
    agentType: TabItem["agentType"]
  ) => void
  bindConversationTab: (
    tabId: string,
    conversationId: number,
    agentType: TabItem["agentType"],
    title: string,
    runtimeConversationId?: number,
    folderId?: number,
    workingDir?: string
  ) => void
  setTabRuntimeConversationId: (
    tabId: string,
    runtimeConversationId: number
  ) => void
  reorderTabs: (reorderedTabs: TabItem[]) => void
  onPreviewTabReplaced: (callback: (tabId: string) => void) => () => void
}

/**
 * Backwards-compatible whole-value accessor over the tab store. Kept so existing
 * consumers keep working during the incremental selector migration; new/hot
 * consumers should read `useTabStore(selector)` / `useTabActions()` directly to
 * subscribe to the narrowest slice they render. `useShallow` keeps the returned
 * object stable, so this re-renders only when a read field (tabs/activeTabId/
 * tabsHydrated/isTileMode) changes — matching the former context's behavior.
 */
export function useTabContext(): TabContextValue {
  return useTabStore(
    useShallow((s) => ({
      tabs: s.tabs,
      activeTabId: s.activeTabId,
      tabsHydrated: s.tabsHydrated,
      isTileMode: s.isTileMode,
      openTab: s.openTab,
      closeTab: s.closeTab,
      closeConversationTab: s.closeConversationTab,
      closeOtherTabs: s.closeOtherTabs,
      closeAllTabs: s.closeAllTabs,
      closeTabsByFolder: s.closeTabsByFolder,
      switchTab: s.switchTab,
      pinTab: s.pinTab,
      toggleTileMode: s.toggleTileMode,
      consumeRemoteActivation: s.consumeRemoteActivation,
      openNewConversationTab: s.openNewConversationTab,
      openChatModeTab: s.openChatModeTab,
      setChatDraftWorkingDir: s.setChatDraftWorkingDir,
      confirmDraftAgent: s.confirmDraftAgent,
      setDraftAgentFromFallback: s.setDraftAgentFromFallback,
      bindConversationTab: s.bindConversationTab,
      setTabRuntimeConversationId: s.setTabRuntimeConversationId,
      reorderTabs: s.reorderTabs,
      onPreviewTabReplaced: s.onPreviewTabReplaced,
    }))
  )
}
