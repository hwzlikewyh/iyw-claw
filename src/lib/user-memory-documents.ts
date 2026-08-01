import type { AgentType } from "@/lib/types"

export type UserMemoryDocumentId = "memory" | "profile" | "soul"

export interface UserMemoryDocumentSnapshot {
  id: UserMemoryDocumentId
  fileName: string
  path: string
  content: string
  etag: string
  enabled: boolean
  readonly: boolean
  readable: boolean
  diagnostic?: string | null
}

export type UserMemoryAvailabilityReason = "root_unavailable"

export interface UserMemoryAvailabilityDiagnostic {
  available: boolean
  reason?: UserMemoryAvailabilityReason | null
  detail?: string | null
}

export type UserMemoryCandidateDiagnosticReason =
  | "root_unavailable"
  | "invalid_state"
  | "read_only"

export interface UserMemoryCandidateDiagnostic {
  available: boolean
  reason?: UserMemoryCandidateDiagnosticReason | null
  detail?: string | null
}

export type CompanionHealthStatus =
  | "ready"
  | "missing"
  | "incompatible"
  | "probe_failed"
  | "timeout"

export interface UserMemoryCompanionHealth {
  status: CompanionHealthStatus
  reason: string
  expectedVersion: string
  detectedVersion?: string | null
  selectedPath?: string | null
  advertisedTools: string[]
  detail?: string | null
}

export interface UserMemoryCapabilityResult {
  available: boolean
  reason: string
  degradedReasons: string[]
}

export interface UserMemoryCapabilities {
  readContext: UserMemoryCapabilityResult
  confirmedAppend: UserMemoryCapabilityResult
  candidateProposal: UserMemoryCapabilityResult
}

export interface UserMemoryMigrationReceipt {
  schemaVersion: number
  consideredSources: Array<{ kind: string; path: string }>
  files: Record<string, { status: string; detail?: string | null }>
  updatedAt: string
}

export interface UserMemoryMigrationReport {
  receipt: UserMemoryMigrationReceipt
  warnings: string[]
}

export interface UserMemorySettingsSnapshot {
  enabled: boolean
  agentWriteEnabled: boolean
  inheritToSubagents: boolean
  perAgent: Record<AgentType, boolean>
  documents: Record<UserMemoryDocumentId, UserMemoryDocumentSnapshot>
  revision: string
  staleRunningSessions: number
  resolvedRoot?: string | null
  rootSource?: string | null
  availability?: UserMemoryAvailabilityDiagnostic
  migrationReport?: UserMemoryMigrationReport | null
  candidateDiagnostic?: UserMemoryCandidateDiagnostic
  candidateCounts?: Partial<Record<UserMemoryCandidateStatus, number>>
  projectedCapabilities?: Partial<Record<AgentType, UserMemoryCapabilities>>
  companionHealth?: UserMemoryCompanionHealth
}

export type UserMemoryCandidateStatus =
  | "tentative"
  | "emerging"
  | "pending_confirmation"
  | "confirmed"
  | "rejected"
  | "superseded"

export type UserMemoryCandidateSignal = "correction" | "preference" | "fact"

export type UserMemoryCandidateStatusFilter =
  | "tentative"
  | "emerging"
  | "pending_confirmation"
  | "terminal"

export interface UserMemoryCandidateSummary {
  id: string
  content: string
  signal: UserMemoryCandidateSignal
  status: UserMemoryCandidateStatus
  observationCount: number
  confidence: number
  wordingVariants: string[]
  sourceAgents: AgentType[]
  firstObservedAt: string
  lastObservedAt: string
  resolvedAt?: string | null
  resolvedContent?: string | null
  confirmedMemoryEntryId?: string | null
  supersededByCandidateId?: string | null
  supersededByMemoryEntryId?: string | null
}

export interface UserMemoryCandidateListRequest {
  status?: UserMemoryCandidateStatusFilter | null
  offset?: number
  limit?: number
}

export interface UserMemoryCandidatePage {
  candidates: UserMemoryCandidateSummary[]
  total: number
  offset: number
  limit: number
  revision: string
}

export type UserMemoryCandidateResolution =
  | { type: "confirm"; editedContent?: string | null }
  | { type: "reject" }
  | { type: "supersede_by_candidate"; candidateId: string }
  | { type: "supersede_by_memory_entry"; entryId: string }

export interface UserMemoryCandidateResolveRequest {
  candidateId: string
  expectedRevision: string
  resolution: UserMemoryCandidateResolution
}

export interface UserMemoryCandidateResolutionResult {
  candidate: UserMemoryCandidateSummary
  revision: string
}

export interface UserMemoryCandidateDeleteRequest {
  candidateId: string
  expectedRevision: string
}

export interface UserMemoryCandidateDeleteResult {
  deleted: boolean
  revision: string
}

export interface UserMemoryHarvestStatus {
  queued: number
  extracting: number
  proposed: number
  noop: number
  failed: number
  dead: number
  backlog: number
  lastHarvestAt?: string | null
  lastSuccessWriteAt?: string | null
  lastFailureAt?: string | null
}

export interface UserMemoryHarvestRescanPreview {
  reQueued: number
  retainedTerminal: number
}

export interface UserMemoryHarvestRescanResult {
  preview: UserMemoryHarvestRescanPreview
  executed: boolean
}

export interface UserMemoryCandidateIndexRebuildResult {
  affected: number
  executed: boolean
  revision: string
}

export const USER_MEMORY_CANDIDATE_STATUS_ORDER: UserMemoryCandidateStatus[] = [
  "tentative",
  "emerging",
  "pending_confirmation",
  "confirmed",
  "rejected",
  "superseded",
]

export interface UserMemoryDocumentUpdate {
  content?: string
  enabled?: boolean
  expectedEtag?: string
}

export interface UserMemoryUpdateRequest {
  expectedRevision: string
  enabled?: boolean
  agentWriteEnabled?: boolean
  inheritToSubagents?: boolean
  perAgent?: Partial<Record<AgentType, boolean>>
  documents?: Partial<Record<UserMemoryDocumentId, UserMemoryDocumentUpdate>>
}

export interface UserMemoryUpdateResult {
  settings: UserMemorySettingsSnapshot
  affectedRunningSessions: number
}

export interface UserMemoryDocumentDraft {
  content: string
  enabled: boolean
}

export interface UserMemoryDraft {
  enabled: boolean
  agentWriteEnabled: boolean
  inheritToSubagents: boolean
  perAgent: Record<AgentType, boolean>
  documents: Record<UserMemoryDocumentId, UserMemoryDocumentDraft>
}

export interface UserMemoryDocument {
  id: UserMemoryDocumentId
  fileName: string
  labelKey:
    | "documents.memory.label"
    | "documents.profile.label"
    | "documents.soul.label"
  descriptionKey:
    | "documents.memory.description"
    | "documents.profile.description"
    | "documents.soul.description"
  placeholderKey:
    | "documents.memory.placeholder"
    | "documents.profile.placeholder"
    | "documents.soul.placeholder"
}

export const USER_MEMORY_DOCUMENTS: UserMemoryDocument[] = [
  {
    id: "memory",
    fileName: "user-memory.md",
    labelKey: "documents.memory.label",
    descriptionKey: "documents.memory.description",
    placeholderKey: "documents.memory.placeholder",
  },
  {
    id: "profile",
    fileName: "user-profile.md",
    labelKey: "documents.profile.label",
    descriptionKey: "documents.profile.description",
    placeholderKey: "documents.profile.placeholder",
  },
  {
    id: "soul",
    fileName: "user-soul.md",
    labelKey: "documents.soul.label",
    descriptionKey: "documents.soul.description",
    placeholderKey: "documents.soul.placeholder",
  },
]

export function getUserMemoryDocument(
  id: UserMemoryDocumentId
): UserMemoryDocument {
  return (
    USER_MEMORY_DOCUMENTS.find((document) => document.id === id) ??
    USER_MEMORY_DOCUMENTS[0]
  )
}

export function createUserMemoryDraft(
  settings: UserMemorySettingsSnapshot
): UserMemoryDraft {
  return {
    enabled: settings.enabled,
    agentWriteEnabled: settings.agentWriteEnabled,
    inheritToSubagents: settings.inheritToSubagents,
    perAgent: { ...settings.perAgent },
    documents: {
      memory: {
        content: settings.documents.memory.content,
        enabled: settings.documents.memory.enabled,
      },
      profile: {
        content: settings.documents.profile.content,
        enabled: settings.documents.profile.enabled,
      },
      soul: {
        content: settings.documents.soul.content,
        enabled: settings.documents.soul.enabled,
      },
    },
  }
}

export function buildUserMemoryUpdateRequest(
  settings: UserMemorySettingsSnapshot,
  draft: UserMemoryDraft
): UserMemoryUpdateRequest | null {
  const request: UserMemoryUpdateRequest = {
    expectedRevision: settings.revision,
  }

  if (draft.enabled !== settings.enabled) request.enabled = draft.enabled
  if (draft.agentWriteEnabled !== settings.agentWriteEnabled) {
    request.agentWriteEnabled = draft.agentWriteEnabled
  }
  if (draft.inheritToSubagents !== settings.inheritToSubagents) {
    request.inheritToSubagents = draft.inheritToSubagents
  }

  const perAgent: Partial<Record<AgentType, boolean>> = {}
  for (const agent of Object.keys(draft.perAgent) as AgentType[]) {
    if (draft.perAgent[agent] !== settings.perAgent[agent]) {
      perAgent[agent] = draft.perAgent[agent]
    }
  }
  if (Object.keys(perAgent).length > 0) request.perAgent = perAgent

  const documents: UserMemoryUpdateRequest["documents"] = {}
  for (const document of USER_MEMORY_DOCUMENTS) {
    const saved = settings.documents[document.id]
    const next = draft.documents[document.id]
    const patch: UserMemoryDocumentUpdate = {}
    if (next.content !== saved.content) {
      patch.content = next.content
      patch.expectedEtag = saved.etag
    }
    if (next.enabled !== saved.enabled) patch.enabled = next.enabled
    if (Object.keys(patch).length > 0) documents[document.id] = patch
  }
  if (Object.keys(documents).length > 0) request.documents = documents

  return Object.keys(request).length === 1 ? null : request
}

export function userMemoryLineCount(content: string): number {
  return content.length === 0 ? 0 : content.split(/\r\n|\r|\n/).length
}

/**
 * True when the document contains machine-managed memory entry markers
 * (`<!-- iyw-memory-... -->` or the legacy `iyw-memory-fallback-...` form).
 * Whole-document saves must not strip these markers (P0-4).
 */
export function userMemoryContainsEntryMarkers(content: string): boolean {
  return /<!--\s*iyw-memory-(?:fallback-)?[0-9a-f]/.test(content)
}
