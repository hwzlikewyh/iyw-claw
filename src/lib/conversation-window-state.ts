import type { ConnectionState } from "@/contexts/acp-connections-context"
import type { TabItem } from "@/stores/tab-store"
import type { ConversationRuntimeSession } from "@/stores/conversation-runtime-store"
import type { ConversationWindowDraft } from "./conversation-window-draft"
import { isDesktop } from "./transport"

export interface ConversationWindowPayload {
  tab: TabItem
  connection: ConnectionState | null
  runtime: ConversationRuntimeSession | null
  draft: ConversationWindowDraft | null
}

export const MOVE_CONVERSATION_WINDOW_EVENT =
  "iyw-claw:move-conversation-window"
export interface ConversationWindowRequest {
  tabId: string
  returnToWorkspace?: boolean
  dropOnly?: boolean
}
const VIRTUAL_ID_HASH_MULTIPLIER = 31
let initialized = false

export function conversationWindowLabel(): string | null {
  if (typeof window === "undefined" || !isDesktop()) return null
  return new URLSearchParams(window.location.search).get("conversationWindow")
}

export function isConversationWindowPending(): boolean {
  return !initialized && conversationWindowLabel() !== null
}

export function isDetachedConversationWindow(): boolean {
  if (typeof window === "undefined" || !isDesktop()) return false
  return (
    new URLSearchParams(window.location.search).get("conversationOnly") === "1"
  )
}

export function markConversationWindowInitialized() {
  initialized = true
}

export function requestConversationWindow(
  tabId: string,
  options?: Omit<ConversationWindowRequest, "tabId">
) {
  if (options?.returnToWorkspace) {
    console.warn(
      "[conversation-window] returning to the workspace is unavailable"
    )
    return
  }
  window.dispatchEvent(
    new CustomEvent(MOVE_CONVERSATION_WINDOW_EVENT, { detail: tabId })
  )
}

export function buildVirtualConversationId(seed: string): number {
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * VIRTUAL_ID_HASH_MULTIPLIER + seed.charCodeAt(i)) | 0
  }
  return -(Math.abs(hash) + 1)
}
