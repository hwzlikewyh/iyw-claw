import { getTransport } from "./transport"
import type { UserMemoryDocumentId } from "./user-memory-documents"

export interface MemoryGovernanceRecommendation {
  id: string
  document: UserMemoryDocumentId
  content: string
  reason: string
  action: "stop" | "recover"
}

export interface MemoryGovernancePreview {
  revision: string
  recommendations: MemoryGovernanceRecommendation[]
  retainedCount: number
}

export function previewMemoryGovernance(): Promise<MemoryGovernancePreview> {
  return getTransport().call("preview_memory_governance")
}

export function applyMemoryGovernance(request: {
  expectedRevision: string
  ids: string[]
  acknowledged: boolean
}): Promise<{ stopped: number; recovered: number; revision: string }> {
  return getTransport().call("apply_memory_governance", { request })
}
