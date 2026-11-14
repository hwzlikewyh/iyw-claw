import type {
  AcpActionsValue,
  ConnectionState,
  ConnectionStoreApi,
} from "@/contexts/acp-connections-context"
import { useTabStore } from "@/stores/tab-store"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import { firstLeafId } from "./tab-group-layout"
import {
  assertReturnCapacity,
  restoreTab as appendRestoredTab,
} from "./conversation-window-restoration"
import {
  captureConversationPanels,
  saveConversationPanels,
  seedConversationPanels,
} from "./conversation-window-panels"
import {
  usePluginComposerContext,
  selectComposerPlugin,
  preparePluginDraft,
} from "./plugin-composer-context"
import {
  buildConversationDraftStorageKey,
  buildNewConversationDraftStorageKey,
} from "./message-input-draft"
import { getActiveRemoteConnectionId, getShellTransport } from "./transport"
import {
  buildVirtualConversationId,
  markConversationWindowInitialized,
  type ConversationWindowPayload,
} from "./conversation-window-state"
import {
  captureConversationWindowDraft,
  seedConversationWindowDraft,
} from "./conversation-window-draft"

const TRANSFER_TIMEOUT_MS = 60_000
const TRANSFER_POLL_MS = 200

interface TransferStatus {
  phase: "preparing" | "ready" | "committed"
  payload: ConversationWindowPayload | null
}

export function getWindowTransfer(label: string) {
  return getShellTransport().call<TransferStatus>(
    "get_conversation_window_transfer",
    { label }
  )
}

async function waitForPhase(label: string, phase: TransferStatus["phase"]) {
  const deadline = Date.now() + TRANSFER_TIMEOUT_MS
  while (Date.now() < deadline) {
    const status = await getWindowTransfer(label)
    if (status.phase === phase) return status
    await new Promise((resolve) => setTimeout(resolve, TRANSFER_POLL_MS))
  }
  throw new Error("Conversation window transfer timed out")
}

function capturePayload(tabId: string, connections: ConnectionStoreApi) {
  const state = useTabStore.getState()
  const tab = state.tabs.find((item) => item.id === tabId)
  if (!tab) throw new Error("Conversation tab was closed")
  const connection = connections.getConnection(tabId) ?? null
  if (connections.isReplacing(tabId) || connection?.status === "connecting") {
    throw new Error("Wait for the conversation connection to finish connecting")
  }
  const runtimeId =
    tab.runtimeConversationId ??
    tab.conversationId ??
    buildVirtualConversationId(`draft-${tab.id}`)
  const runtime =
    useConversationRuntimeStore.getState().byConversationId.get(runtimeId) ??
    null
  const folder = useAppWorkspaceStore
    .getState()
    .allFolders.find((item) => item.id === tab.folderId)
  return {
    tab: { ...tab, workingDir: tab.workingDir ?? folder?.path, isPinned: true },
    sidebar: {
      isOpen: false,
      width: 320,
      activeTab: "home",
      apps: [],
      ...captureConversationPanels(tab.conversationId ?? null),
      ...usePluginComposerContext.getState().entries[tabId],
    },
    connection: connection?.status === "disconnected" ? null : connection,
    runtime,
    draft: captureConversationWindowDraft(
      tabId,
      tab.conversationId != null
        ? buildConversationDraftStorageKey(tab.conversationId)
        : buildNewConversationDraftStorageKey(tabId)
    ),
  } satisfies ConversationWindowPayload
}

async function rollbackTransfer(label: string | null) {
  if (!label) return
  await getShellTransport().call("finish_conversation_window", { label })
}

interface MoveOptions {
  tabId: string
  connections: ConnectionStoreApi
  actions: AcpActionsValue
}

async function commitPreparedWindow(
  options: MoveOptions,
  label: string,
  payload: ConversationWindowPayload
) {
  const latest = capturePayload(options.tabId, options.connections)
  if (
    latest.connection?.connectionId !== payload.connection?.connectionId ||
    latest.tab.folderId !== payload.tab.folderId
  ) {
    throw new Error(
      "The conversation connection changed. Please try moving it again."
    )
  }
  // 在后端交接前撤销来源的清理权限，避免来源卸载取消目标任务。
  options.actions.setWindowConnectionViewer(options.tabId, true)
  await getShellTransport().call("commit_conversation_window", {
    label,
    snapshot: {
      tab: latest.tab,
      runtime: latest.runtime,
      draft: latest.draft,
      sidebar: latest.sidebar,
    },
  })
}

export async function moveConversationWindow(options: MoveOptions) {
  const { tabId, connections, actions } = options
  const payload = capturePayload(tabId, connections)
  await saveConversationPanels(payload.sidebar ?? null)
  actions.touchActivity(tabId)
  let label: string | null = null
  let committed = false
  try {
    label = await getShellTransport().call<string>("open_conversation_window", {
      payload,
      remoteConnectionId: getActiveRemoteConnectionId(),
    })
    await waitForPhase(label, "ready")
    await commitPreparedWindow(options, label, payload)
    committed = true
    actions.releaseWindowConnection(tabId)
    useTabStore.getState().closeTab(tabId)
    console.info("[conversation-window] source released", { label, tabId })
  } catch (error) {
    if (!committed) {
      actions.setWindowConnectionViewer(
        tabId,
        payload.connection?.isViewer ?? false
      )
      await rollbackTransfer(label).catch((cleanupError) => {
        console.error("[conversation-window] rollback failed", cleanupError)
      })
    }
    throw error
  }
}

function restoreRuntime(payload: ConversationWindowPayload) {
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

function restoreTab(payload: ConversationWindowPayload) {
  const state = useTabStore.getState()
  const group = firstLeafId(state.groupLayout)
  const tab = { ...payload.tab, agentTypeProvisional: false }
  useTabStore.setState({
    rawTabs: [tab],
    tabs: [tab],
    activeTabId: tab.id,
    tabsHydrated: true,
    groupOf: { [tab.id]: group },
    groupSelection: { [group]: tab.id },
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

function waitForConnectionSnapshot(
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

export async function restoreConversationWindow(
  label: string,
  actions: AcpActionsValue,
  target:
    | ConnectionStoreApi
    | { connections: ConnectionStoreApi; append?: boolean }
) {
  const append = "connections" in target && target.append === true
  const connections = "connections" in target ? target.connections : target
  const { payload } = await getWindowTransfer(label)
  if (!payload?.tab?.id) throw new Error("Conversation window state is missing")
  if (append) assertReturnCapacity(payload)
  const workspace = useAppWorkspaceStore.getState()
  if (
    payload.tab.folderId > 0 &&
    !workspace.folders.some((folder) => folder.id === payload.tab.folderId)
  ) {
    await workspace.addFolderToWorkspaceById(payload.tab.folderId)
  }
  restoreRuntime(payload)
  seedConversationWindowDraft(payload.tab.id, payload.draft)
  if (payload.connection) {
    await actions.attachWindowConnection(payload.connection)
    await waitForConnectionSnapshot(connections, payload.connection)
  }
  await getShellTransport().call("ready_conversation_window")
  const committed = await waitForPhase(label, "committed")
  const restored = committed.payload ?? payload
  restoreRuntime(restored)
  seedConversationWindowDraft(restored.tab.id, restored.draft)
  actions.setWindowConnectionViewer(
    restored.tab.id,
    !!(restored.connection?.isViewer || restored.connection?.isDelegationChild)
  )
  if (append) appendRestoredTab(restored, true)
  else restoreTab(restored)
  seedConversationPanels(restored.sidebar ?? null)
  if (restored.sidebar?.selected)
    selectComposerPlugin(restored.tab.id, restored.sidebar.selected)
  if (restored.sidebar?.preparedConversationId)
    preparePluginDraft(restored.tab.id, restored.sidebar.preparedConversationId)
  markConversationWindowInitialized()
  await finishRestoration(label, restored.tab.id)
}

async function finishRestoration(label: string, tabId: string) {
  await getShellTransport()
    .call("finish_conversation_window", { label })
    .catch((error) =>
      console.warn("[conversation-window] receipt cleanup failed", error)
    )
  // 刷新独立窗口时恢复普通工作区启动流程，不重复消费已提交的交接。
  const url = new URL(window.location.href)
  url.searchParams.delete("conversationWindow")
  try {
    window.history.replaceState(window.history.state, "", url)
  } catch (error) {
    console.warn("[conversation-window] URL cleanup failed", error)
  }
  console.info("[conversation-window] target restored", {
    label,
    tabId,
  })
}
