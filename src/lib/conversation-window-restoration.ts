import type {
  ConnectionState,
  ConnectionStoreApi,
} from "@/contexts/acp-connections-context"
import { useTabStore } from "@/stores/tab-store"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import { firstLeafId } from "./tab-group-layout"
import type { ConversationWindowPayload } from "./conversation-window-state"
import {
  canOpenConversationTab,
  conversationTabLimitMessage,
} from "./conversation-tab-preferences"
const TRANSFER_TIMEOUT_MS = 60_000

export function assertReturnCapacity(payload: ConversationWindowPayload) {
  const state = useTabStore.getState()
  if (
    state.rawTabs.some(
      (tab) =>
        tab.id === payload.tab.id ||
        (payload.tab.conversationId != null &&
          tab.conversationId === payload.tab.conversationId &&
          tab.folderId === payload.tab.folderId &&
          tab.agentType === payload.tab.agentType)
    )
  ) {
    throw new Error(
      "This conversation is already open in the original workspace"
    )
  }
  if (!canOpenConversationTab(state.rawTabs.length))
    throw new Error(conversationTabLimitMessage())
}

export function restoreRuntime(payload: ConversationWindowPayload) {
  const session = payload.runtime
  if (!session) return
  useConversationRuntimeStore.setState((state) => {
    const byConversationId = new Map(state.byConversationId)
    byConversationId.set(session.conversationId, {
      ...session,
      detailLoading: false,
      pendingCleanup: false,
    })
    const conversationIdByExternalId = new Map(state.conversationIdByExternalId)
    if (session.externalId) {
      conversationIdByExternalId.set(session.externalId, session.conversationId)
    }
    return { byConversationId, conversationIdByExternalId }
  })
}

export function restoreTab(
  payload: ConversationWindowPayload,
  append: boolean
) {
  const state = useTabStore.getState()
  const group = firstLeafId(state.groupLayout)
  const tab = { ...payload.tab, agentTypeProvisional: false }
  useTabStore.setState({
    rawTabs: append ? [...state.rawTabs, tab] : [tab],
    tabs: append ? [...state.tabs, tab] : [tab],
    activeTabId: tab.id,
    tabsHydrated: true,
    groupOf: { ...(append ? state.groupOf : {}), [tab.id]: group },
    groupSelection: {
      ...(append ? state.groupSelection : {}),
      [group]: tab.id,
    },
  })
  useTabStore.getState().switchTab(tab.id)
}

function isTransferredConnectionReady(
  current: ConnectionState,
  expected: ConnectionState
) {
  const metadataReady =
    (!expected.sessionId || !!current.sessionId) &&
    (!expected.selectorsReady || current.selectorsReady)
  return (
    metadataReady &&
    (current.status === "connected" || current.status === "prompting")
  )
}

export function waitForConnectionSnapshot(
  store: ConnectionStoreApi,
  expected: ConnectionState
) {
  return new Promise<void>((resolve, reject) => {
    let unsubscribe = () => {}
    const finish = (error?: Error) => {
      clearTimeout(timer)
      unsubscribe()
      if (error) reject(error)
      else resolve()
    }
    const timer = setTimeout(
      () => finish(new Error("Conversation snapshot timed out")),
      TRANSFER_TIMEOUT_MS
    )
    const check = () => {
      const current = store.getConnection(expected.contextKey)
      if (
        !current ||
        current.connectionId !== expected.connectionId ||
        current.status === "disconnected"
      ) {
        finish(new Error("Conversation connection ended during transfer"))
      } else if (isTransferredConnectionReady(current, expected)) {
        finish()
      }
    }
    unsubscribe = store.subscribeKey(expected.contextKey, check)
    check()
  })
}
