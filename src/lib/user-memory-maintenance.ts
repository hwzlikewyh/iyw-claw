import { getTransport } from "./transport"

export interface MemoryReview {
  id: string
  target: { id: string; contentDigest: string }
  content: string
  reason: string
  quote: string
  status: "pending" | "applied" | "dismissed"
  createdAt: string
  resolvedAt: string | null
}

export interface MemoryMaintenanceStatus {
  revision: string
  busy: boolean
  lastCheckedAt: string | null
  activeCount: number
  expiredCount: number
  staleReviewIds: string[]
  modelReview: {
    lastAttemptAt: string | null
    lastCompletedAt: string | null
    lastErrorCode: string | null
    reviews: MemoryReview[]
  }
}

export interface MemoryMigrationPreview {
  sourceRevision: string
  generatedAt: string
  counts: Record<string, number>
  warnings: string[]
  unparsedLines: MemoryMigrationIssue[]
  readyForShadowImport: boolean
  records: {
    id: string
    kind: string
    content: string | null
    state: string
    scopeType: string
    scopeKey: string
    validTo: string | null
  }[]
}

export interface MemoryMigrationIssue {
  lineNumber: number
  content: string | null
  contentDigest: string
  sensitive: boolean
}

export interface ReconcileMemoryMigrationResult {
  converted: number
  backupFile: string | null
}

export const getMemoryMaintenance = () =>
  getTransport().call<MemoryMaintenanceStatus>("get_user_memory_maintenance")

export const runMemoryMaintenance = () =>
  getTransport().call<void>("run_user_memory_maintenance")

export const resolveMemoryReview = (request: {
  id: string
  expectedRevision: string
  apply: boolean
}) => getTransport().call<void>("resolve_user_memory_review", { request })

export const previewMemoryMigration = () =>
  getTransport().call<MemoryMigrationPreview>("preview_user_memory_migration")

export const reconcileMemoryMigration = (expectedRevision: string) =>
  getTransport().call<ReconcileMemoryMigrationResult>(
    "reconcile_user_memory_migration",
    { request: { expectedRevision } }
  )
