import type { JSONContent } from "@tiptap/core"
import {
  buildConversationDraftStorageKey,
  buildNewConversationDraftStorageKey,
  loadMessageInputDraftV2,
} from "@/lib/message-input-draft"
import { isAgentType, type AgentType } from "@/lib/types"

const RECEIPT_PREFIX = "iyw-claw:native-share-delivery:v2:"
const LEGACY_RECEIPT_PREFIX = "iyw-claw:native-share-delivery:v1:"

export interface ShareDraftReceipt {
  tabId: string
  agentType: AgentType
  saved: boolean
}

export const nativeShareDraftId = (batchId: string) => `new-share-${batchId}`

export function readShareDraftReceipt(
  batchId: string
): ShareDraftReceipt | null {
  const raw = localStorage.getItem(`${RECEIPT_PREFIX}${batchId}`)
  if (!raw) return null
  const value: unknown = JSON.parse(raw)
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Invalid native share draft receipt")
  }
  const { tabId, agentType, saved } = value as Partial<ShareDraftReceipt>
  if (
    tabId !== nativeShareDraftId(batchId) ||
    !isAgentType(agentType) ||
    typeof saved !== "boolean"
  ) {
    throw new Error("Invalid native share draft receipt")
  }
  return { tabId, agentType, saved }
}

export function saveShareDraftReceipt(
  batchId: string,
  receipt: ShareDraftReceipt
) {
  localStorage.setItem(`${RECEIPT_PREFIX}${batchId}`, JSON.stringify(receipt))
}

export function hasNativeShareReference(
  node: JSONContent,
  batchId: string
): boolean {
  return (
    (node.type === "reference" &&
      String(node.attrs?.id).startsWith(`native-share:${batchId}:`)) ||
    (node.content?.some((child) => hasNativeShareReference(child, batchId)) ??
      false)
  )
}

export function readLegacyShareDraft(batchId: string): JSONContent | null {
  const raw = localStorage.getItem(`${LEGACY_RECEIPT_PREFIX}${batchId}`)
  if (!raw) return null
  const receipt = JSON.parse(raw) as { conversationId?: number }
  const id = receipt?.conversationId
  if (typeof id !== "number" || !Number.isSafeInteger(id) || id <= 0)
    return null
  const draft = loadMessageInputDraftV2(buildConversationDraftStorageKey(id))
  return draft?.kind === "doc" && hasNativeShareReference(draft.doc, batchId)
    ? draft.doc
    : null
}

export function hasRecoverableShareDraft(batchId: string): boolean {
  try {
    const receipt = readShareDraftReceipt(batchId)
    return receipt
      ? loadMessageInputDraftV2(
          buildNewConversationDraftStorageKey(receipt.tabId)
        ) !== null
      : readLegacyShareDraft(batchId) !== null
  } catch {
    // 自动恢复只读取有效回执；损坏的待导入批次仍由正常交付路径报告错误。
    return false
  }
}
