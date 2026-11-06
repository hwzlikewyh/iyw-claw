import {
  assertReturnCapacity,
  restoreRuntime,
  restoreTab,
  waitForConnectionSnapshot,
} from "./conversation-window-restoration"
import type {
  AcpActionsValue,
  ConnectionStoreApi,
} from "@/contexts/acp-connections-context"
import { useTabStore } from "@/stores/tab-store"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import {
  buildConversationDraftStorageKey,
  buildNewConversationDraftStorageKey,
} from "./message-input-draft"
import { getActiveRemoteConnectionId, getShellTransport } from "./transport"
import {
  buildVirtualConversationId,
  markConversationWindowInitialized,
  type ConversationWindowPayload,
  type ConversationWindowRequest,
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

interface MoveOptions extends ConversationWindowRequest {
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
    snapshot: { tab: latest.tab, runtime: latest.runtime, draft: latest.draft },
  })
}

export async function moveConversationWindow(options: MoveOptions) {
  const { tabId, connections, actions } = options
  const payload = capturePayload(tabId, connections)
  actions.touchActivity(tabId)
  let label: string | null = null
  let committed = false
  try {
    label = await prepareWindow(options, payload)
    if (!label) return
    await waitForPhase(label, "ready")
    await commitPreparedWindow(options, label, payload)
    committed = true
    actions.releaseWindowConnection(tabId)
    await releaseSourceWindow(options)
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

function prepareWindow(
  options: MoveOptions,
  payload: ConversationWindowPayload
) {
  return options.returnToWorkspace
    ? getShellTransport().call<string | null>("return_conversation_window", {
        payload,
        dropOnly: options.dropOnly ?? false,
      })
    : getShellTransport().call<string>("open_conversation_window", {
        payload,
        remoteConnectionId: getActiveRemoteConnectionId(),
      })
}

async function releaseSourceWindow(options: MoveOptions) {
  if (!options.returnToWorkspace) {
    useTabStore.getState().closeTab(options.tabId)
    return
  }
  const { getCurrentWindow } = await import("@tauri-apps/api/window")
  await getCurrentWindow().close()
}

async function prepareRestoration(
  payload: ConversationWindowPayload,
  actions: AcpActionsValue,
  target: { connections: ConnectionStoreApi; append?: boolean }
) {
  if (target.append) assertReturnCapacity(payload)
  const workspace = useAppWorkspaceStore.getState()
  const folderId = payload.tab.folderId
  if (
    folderId > 0 &&
    !workspace.folders.some((folder) => folder.id === folderId)
  ) {
    await workspace.addFolderToWorkspaceById(folderId)
  }
  restoreRuntime(payload)
  seedConversationWindowDraft(payload.tab.id, payload.draft)
  if (payload.connection) {
    await actions.attachWindowConnection(payload.connection)
    await waitForConnectionSnapshot(target.connections, payload.connection)
  }
}

export async function restoreConversationWindow(
  label: string,
  actions: AcpActionsValue,
  target: { connections: ConnectionStoreApi; append?: boolean }
) {
  const { payload } = await getWindowTransfer(label)
  if (!payload?.tab?.id) throw new Error("Conversation window state is missing")
  let committed: TransferStatus
  try {
    await prepareRestoration(payload, actions, target)
    await getShellTransport().call("ready_conversation_window", { label })
    committed = await waitForPhase(label, "committed")
  } catch (error) {
    seedConversationWindowDraft(payload.tab.id, null)
    if (
      !useTabStore.getState().rawTabs.some((tab) => tab.id === payload.tab.id)
    ) {
      actions.releaseWindowConnection(payload.tab.id)
    }
    throw error
  }
  const restored = committed.payload ?? payload
  restoreRuntime(restored)
  seedConversationWindowDraft(restored.tab.id, restored.draft)
  actions.setWindowConnectionViewer(
    restored.tab.id,
    !!(restored.connection?.isViewer || restored.connection?.isDelegationChild)
  )
  restoreTab(restored, target.append ?? false)
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
