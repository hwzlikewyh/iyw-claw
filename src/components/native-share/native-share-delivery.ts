import type { JSONContent } from "@tiptap/core"
import { useTabStore } from "@/stores/tab-store"
import { conversationTabLimitMessage } from "@/lib/conversation-tab-preferences"
import { isRemoteDesktopMode } from "@/lib/transport"
import { requireFileUploadCapability } from "@/lib/transport/capability-policy"
import {
  buildNewConversationDraftStorageKey,
  loadMessageInputDraftV2,
  persistMessageInputDraftV2Now,
} from "@/lib/message-input-draft"
import { buildFileUri } from "@/lib/reference-link"
import {
  completeNativeShare,
  prepareNativeShare,
  type SharePreview,
} from "@/lib/native-share"
import type { AgentType } from "@/lib/types"
import {
  hasNativeShareReference,
  nativeShareDraftId,
  readLegacyShareDraft,
  readShareDraftReceipt,
  saveShareDraftReceipt,
  type ShareDraftReceipt,
} from "./native-share-draft-receipt"

interface DeliveryOptions {
  batchId: string
  agentType: AgentType
  prompt: string
  remoteUnsupported: string
}

function requireLocal(message: string) {
  if (isRemoteDesktopMode()) throw new Error(message)
}

function saveDraft(
  receipt: ShareDraftReceipt,
  preview: SharePreview,
  prompt: string
) {
  const key = buildNewConversationDraftStorageKey(receipt.tabId)
  const existing = loadMessageInputDraftV2(key)
  // 已发送或被用户清空的草稿不能因确认回执重试而重新出现。
  if (receipt.saved) return existing !== null
  const legacy = readLegacyShareDraft(preview.id)
  const doc: JSONContent =
    existing?.kind === "doc"
      ? { ...existing.doc, content: [...(existing.doc.content ?? [])] }
      : legacy
        ? { ...legacy, content: [...(legacy.content ?? [])] }
        : { type: "doc", content: [] }
  if (!hasNativeShareReference(doc, preview.id)) {
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
  saveShareDraftReceipt(preview.id, receipt)
  return true
}

export async function deliverNativeShare(
  options: DeliveryOptions
): Promise<void> {
  requireLocal(options.remoteUnsupported)
  await requireFileUploadCapability()
  const preview = await prepareNativeShare(options.batchId)
  requireLocal(options.remoteUnsupported)
  const receipt = readShareDraftReceipt(preview.id) ?? {
    tabId: nativeShareDraftId(preview.id),
    agentType: options.agentType,
    saved: false,
  }
  // 先保存，再打开首页草稿；由正常首次发送流程创建数据库会话与运行时。
  if (saveDraft(receipt, preview, options.prompt)) {
    useTabStore.getState().openChatModeTab({
      independentDraft: { id: receipt.tabId, agentType: receipt.agentType },
    })
    const opened = useTabStore
      .getState()
      .rawTabs.some((tab) => tab.id === receipt.tabId)
    if (!opened) throw new Error(conversationTabLimitMessage())
  }
  await completeNativeShare(preview.id)
  console.info("[NativeShare] delivered new conversation draft", {
    batchId: preview.id,
    tabId: receipt.tabId,
  })
}
