import type { JSONContent } from "@tiptap/core"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useTabStore } from "@/stores/tab-store"
import { conversationTabLimitMessage } from "@/lib/conversation-tab-preferences"
import { getShellTransport, isRemoteDesktopMode } from "@/lib/transport"
import { requireFileUploadCapability } from "@/lib/transport/capability-policy"
import {
  buildConversationDraftStorageKey,
  loadMessageInputDraftV2,
  persistMessageInputDraftV2Now,
} from "@/lib/message-input-draft"
import { buildFileUri } from "@/lib/reference-link"
import {
  completeNativeShare,
  prepareNativeShare,
  NATIVE_SHARE_OPENED,
  type SharePreview,
} from "@/lib/native-share"
import {
  isAgentType,
  type AgentType,
  type CreateChatConversationResult,
  type FolderDetail,
} from "@/lib/types"

const RECEIPT_PREFIX = "iyw-claw:native-share-delivery:v1:"

interface DeliveryReceipt {
  conversationId: number
  folderId: number
  agentType: AgentType
  saved: boolean
}

interface DeliveryOptions {
  batchId: string
  agentType: AgentType
  title: string
  prompt: string
  remoteUnsupported: string
}

function requireLocal(message: string) {
  if (isRemoteDesktopMode()) throw new Error(message)
}

function readReceipt(key: string): DeliveryReceipt | null {
  const raw = localStorage.getItem(key)
  if (!raw) return null
  const value = JSON.parse(raw) as DeliveryReceipt
  if (
    !Number.isSafeInteger(value.conversationId) ||
    value.conversationId <= 0 ||
    !Number.isSafeInteger(value.folderId) ||
    value.folderId <= 0 ||
    !isAgentType(value.agentType) ||
    typeof value.saved !== "boolean"
  ) {
    throw new Error("Invalid native share delivery receipt")
  }
  return value
}

async function resolveReceipt(
  options: DeliveryOptions
): Promise<DeliveryReceipt> {
  const key = `${RECEIPT_PREFIX}${options.batchId}`
  const existing = readReceipt(key)
  if (existing) return existing
  const created = await getShellTransport().call<CreateChatConversationResult>(
    "create_chat_conversation",
    { agentType: options.agentType, title: options.title, existingDir: null }
  )
  const receipt: DeliveryReceipt = {
    conversationId: created.conversationId,
    folderId: created.folderId,
    agentType: options.agentType,
    saved: false,
  }
  localStorage.setItem(key, JSON.stringify(receipt))
  return receipt
}

function containsBatch(node: JSONContent, id: string): boolean {
  return (
    (node.type === "reference" &&
      String(node.attrs?.id).startsWith(`native-share:${id}:`)) ||
    (node.content?.some((child) => containsBatch(child, id)) ?? false)
  )
}

function saveDraft(
  receipt: DeliveryReceipt,
  preview: SharePreview,
  prompt: string
) {
  if (receipt.saved) return
  const key = buildConversationDraftStorageKey(receipt.conversationId)
  const existing = loadMessageInputDraftV2(key)
  const doc: JSONContent =
    existing?.kind === "doc"
      ? { ...existing.doc, content: [...(existing.doc.content ?? [])] }
      : { type: "doc", content: [] }
  if (!containsBatch(doc, preview.id)) {
    const content: JSONContent[] = [{ type: "text", text: prompt }]
    preview.paths.forEach((path, index) =>
      content.push(
        { type: "text", text: " " },
        {
          type: "reference",
          attrs: {
            refType: "file",
            id: `native-share:${preview.id}:${index}`,
            label: path.split(/[/\\]/).pop() || path,
            uri: buildFileUri(path),
            meta: { fileKind: "file" },
          },
        }
      )
    )
    doc.content = [...(doc.content ?? []), { type: "paragraph", content }]
  }
  persistMessageInputDraftV2Now(key, doc)
  receipt.saved = true
  localStorage.setItem(
    `${RECEIPT_PREFIX}${preview.id}`,
    JSON.stringify(receipt)
  )
}

export async function deliverNativeShare(
  options: DeliveryOptions
): Promise<void> {
  requireLocal(options.remoteUnsupported)
  await requireFileUploadCapability()
  const preview = await prepareNativeShare(options.batchId)
  requireLocal(options.remoteUnsupported)
  const receipt = await resolveReceipt(options)
  const folder = await getShellTransport().call<FolderDetail>("get_folder", {
    folderId: receipt.folderId,
  })
  requireLocal(options.remoteUnsupported)
  // 先持久化，再挂载会话；不依赖编辑器初始化时序，也不触碰当前输入框。
  saveDraft(receipt, preview, options.prompt)
  useAppWorkspaceStore.getState().upsertFolder(folder)
  useTabStore
    .getState()
    .openTab(
      receipt.folderId,
      receipt.conversationId,
      receipt.agentType,
      true,
      options.title
    )
  const opened = useTabStore
    .getState()
    .rawTabs.some(
      (tab) =>
        tab.conversationId === receipt.conversationId &&
        tab.folderId === receipt.folderId &&
        tab.agentType === receipt.agentType
    )
  if (!opened) throw new Error(conversationTabLimitMessage())
  window.dispatchEvent(new Event(NATIVE_SHARE_OPENED))
  await completeNativeShare(preview.id)
  console.info("[NativeShare] delivered new conversation draft", {
    batchId: preview.id,
    conversationId: receipt.conversationId,
  })
}
