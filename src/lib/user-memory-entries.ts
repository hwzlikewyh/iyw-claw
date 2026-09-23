import { getTransport } from "./transport"
import type { UserMemoryDocumentId } from "./user-memory-documents"

export interface UserMemoryEntry {
  id: string
  content: string
  active: boolean
  expiresAt: string | null
  sourceRevision: string
  sources: string[]
  scopeType: string
  scopeKey: string
  generated: boolean
}

export interface UserMemoryEntryPage {
  entries: UserMemoryEntry[]
  total: number
  revision: string
  documentEtag: string
  readonly: boolean
}

export interface MemorySemanticStatus {
  config: {
    embeddingModel: string
    rerankModel: string
    rerankEnabled: boolean
  }
  recallEnabled: boolean
  supported: boolean
  ready: boolean
  busy: boolean
  indexedItems: number
  lastError: string | null
}

export function previewMemorySemantic(query: string): Promise<{
  items: { id: string; content: string; kind: string; score: number }[]
  status: MemorySemanticStatus
}> {
  return getTransport().call("preview_user_memory_semantic", { query })
}

export function listUserMemoryEntries(request: {
  document: UserMemoryDocumentId
  query: string
  offset: number
  includeInactive: boolean
}): Promise<UserMemoryEntryPage> {
  return getTransport().call("list_user_memory_entries", { request })
}

export function setUserMemoryEntryStatus(request: {
  id: string
  document: UserMemoryDocumentId
  expectedRevision: string
  active: boolean
}): Promise<void> {
  return getTransport().call("set_user_memory_entry_status", { request })
}

export interface ForgetUserMemoryResult {
  forgotten: boolean
  revision: string
  purgedBackupPaths: string[]
  residualBackupPaths: string[]
}

export function forgetUserMemory(request: {
  id: string
  document: UserMemoryDocumentId
  expectedRevision: string
  purgeBackups: boolean
  confirmation: string
}): Promise<ForgetUserMemoryResult> {
  return getTransport().call("forget_user_memory", { request })
}

export type ClearUserMemoryScope = "memory" | "all"

export interface ClearUserMemoryResult {
  clearedRecords: number
  purgedBackupPaths: string[]
  residualBackupPaths: string[]
  residualFilePaths: string[]
}

export function clearUserMemory(request: {
  scope: ClearUserMemoryScope
  expectedRevision: string
  purgeBackups: boolean
  confirmation: string
}): Promise<ClearUserMemoryResult> {
  return getTransport().call("clear_user_memory", { request })
}
