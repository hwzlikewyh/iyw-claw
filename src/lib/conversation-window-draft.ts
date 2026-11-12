import type { JSONContent } from "@tiptap/core"
import type { InputAttachment } from "@/components/chat/message-input-attachments"
import type { PromptInputBlock } from "@/lib/types"
import {
  loadMessageInputDraftV2,
  persistMessageInputDraftV2Now,
} from "./message-input-draft"

export interface ConversationWindowDraft {
  doc: JSONContent | null
  attachments: InputAttachment[]
  embeddedPayloads: [string, PromptInputBlock][]
}

const captures = new Map<string, () => ConversationWindowDraft>()
const incoming = new Map<string, ConversationWindowDraft>()

export function registerConversationWindowDraft(
  tabId: string,
  capture: () => ConversationWindowDraft
) {
  captures.set(tabId, capture)
  return () => {
    if (captures.get(tabId) === capture) captures.delete(tabId)
  }
}

export function captureConversationWindowDraft(
  tabId: string,
  draftKey: string
) {
  const capture = captures.get(tabId)
  if (capture) return capture()
  // 尚未挂载的编辑器通过现有草稿恢复路径读取，先刷出来源的内存缓存。
  const loaded = loadMessageInputDraftV2(draftKey)
  if (loaded?.kind === "doc")
    persistMessageInputDraftV2Now(draftKey, loaded.doc)
  return null
}

export function seedConversationWindowDraft(
  tabId: string,
  draft: ConversationWindowDraft | null
) {
  if (draft) incoming.set(tabId, draft)
  else incoming.delete(tabId)
}

export function takeConversationWindowDraft(tabId: string | null | undefined) {
  if (!tabId) return null
  const draft = incoming.get(tabId) ?? null
  incoming.delete(tabId)
  return draft
}
