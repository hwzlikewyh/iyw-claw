import { getTransport } from "./transport"
import type { UserMemoryDocumentId } from "./user-memory-documents"

export interface MemoryFileConflict {
  name: string
  document: UserMemoryDocumentId | null
  current: string | null
  external: string | null
  currentDigest: string | null
  externalDigest: string | null
  redacted: boolean
  importAllowed: boolean
  importError: string | null
}

export interface MemoryReconciliation {
  revision: string
  mode: "healthy" | "external_conflict" | "restore_required"
  databaseEpoch: number | null
  requiredEpoch: number | null
  files: MemoryFileConflict[]
  recoverySources: {
    id: string
    label: string
    epoch: number
    records: number
    revisions: number
  }[]
  warnings: string[]
}

export type ConflictAction = "keep_current" | "import_paused"
export type ReconciliationResult = { backupPath: string }

export const getMemoryReconciliation = () =>
  getTransport().call<MemoryReconciliation>("get_user_memory_reconciliation")
export const resolveMemoryFile = (request: {
  expectedRevision: string
  name: string
  action: ConflictAction
}) =>
  getTransport().call<ReconciliationResult>("resolve_user_memory_file", {
    request,
  })
export const restoreMemoryAuthority = (request: {
  expectedRevision: string
  sourceId: string
}) =>
  getTransport().call<ReconciliationResult>("restore_user_memory_authority", {
    request,
  })
