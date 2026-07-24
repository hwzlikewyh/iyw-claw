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

export interface UserMemorySettingsSnapshot {
  enabled: boolean
  agentWriteEnabled: boolean
  inheritToSubagents: boolean
  perAgent: Record<AgentType, boolean>
  documents: Record<UserMemoryDocumentId, UserMemoryDocumentSnapshot>
  revision: string
  staleRunningSessions: number
}

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
