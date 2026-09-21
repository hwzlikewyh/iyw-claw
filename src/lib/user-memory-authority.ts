import { getTransport } from "./transport"
import type { AgentType } from "./types"

export interface MemoryAuthorityStatus {
  mode: "legacy" | "shadow" | "active"
  epoch: number | null
  revision: string | null
  records: number
  revisions: number
  pendingProjections: number
  backupPath: string | null
  externalChanges: string[]
  counts: Record<string, number>
}

export interface MemoryRevisionEntry {
  recordId: string
  sourceId: string
  revision: number
  state: string
  reason: string
  recordedAt: string
  item: {
    redacted?: boolean
    item?: {
      content: string
      valid_to: string | null
      scope_type: string
      scope_key: string
    }
    relations?: { relation: string; target_id: string }[]
  }
  sources: {
    sourceId: string
    turnNonce: number
    availability: "available" | "deleted_or_missing" | "unresolved"
    conversation: {
      id: number
      folderId: number
      agentType: AgentType
      title: string | null
    } | null
  }[]
}

export const getMemoryAuthority = () =>
  getTransport().call<MemoryAuthorityStatus>("get_user_memory_authority")
export const prepareMemoryAuthority = () =>
  getTransport().call<MemoryAuthorityStatus>("prepare_user_memory_authority")
export const activateMemoryAuthority = (expectedRevision: string) =>
  getTransport().call<MemoryAuthorityStatus>("activate_user_memory_authority", {
    request: { expectedRevision },
  })
export const getMemoryHistory = (id: string) =>
  getTransport().call<MemoryRevisionEntry[]>("get_user_memory_history", { id })

export interface MemoryRecallReceipt {
  id: number
  conversationId: string | null
  turnNonce: number | null
  recordedAt: string
  items: {
    recordId: string
    sourceId: string
    revision: number
    sourceRevision: string
  }[]
  feedback: "used" | "irrelevant" | "outdated" | null
}

export const getMemoryReceipts = (id: string) =>
  getTransport().call<MemoryRecallReceipt[]>("get_user_memory_receipts", { id })

export const recordMemoryRecallFeedback = (request: {
  receiptId: number
  recordId: string
  verdict: "used" | "irrelevant" | "outdated"
  note?: string | null
}) => getTransport().call<void>("record_memory_recall_feedback", { request })

export interface MemoryEffectivenessStatus {
  deliveries: number
  reviewed: number
  used: number
  irrelevant: number
  outdated: number
  unreviewed: number
}

export const getMemoryEffectiveness = () =>
  getTransport().call<MemoryEffectivenessStatus>("get_memory_effectiveness")
