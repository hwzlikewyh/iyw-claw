import type { AgentType } from "./types"
import type { UserMemoryDocumentId } from "./user-memory-documents"

export interface AppendUserMemoryRequest {
  content: string
  agentType: AgentType
}

export interface UserMemoryAppendResult {
  appended: boolean
  entryId: string
  createdAt: string
  revision: string
}

export interface CorrectUserMemoryRequest {
  document: UserMemoryDocumentId
  oldContent: string
  newContent: string
  expectedEtag: string
}

export interface CorrectUserMemoryResult {
  document: UserMemoryDocumentId
  oldEntryId: string
  newEntryId: string
  revision: string
}
