import type { AgentType } from "@/lib/types"

export interface ShareBlock {
  type: "text" | "file"
  text?: string
  reference?: string
}
export interface ShareMessage {
  id: string
  role: "user" | "assistant"
  timestamp: string
  blocks: ShareBlock[]
}
export interface ShareSnapshot {
  version: 1
  agentType: AgentType
  messages: ShareMessage[]
}
export interface ShareFileSpec {
  reference: string
  messageId: string
  fileName: string
  contentType: string
  sizeBytes: number
  sha256: string
}
export interface ShareFile extends ShareFileSpec {
  id: string
}
export interface ShareRecord {
  id: string
  title: string
  snapshot: ShareSnapshot
  files: ShareFile[]
  state: "draft" | "published" | "revoked"
  expirationDays: number
  publishedAt: string | null
  expiresAt: string | null
  createdAt: string
}
export interface ShareUpload {
  reference: string
  url: string
  headers: Record<string, string>
}
export interface ShareCreated {
  share: ShareRecord
  token: string
  uploads: ShareUpload[]
}
export interface ShareCandidate {
  reference: string
  messageId: string
  name: string
  path?: string
  uri?: string
  file?: File
  data?: string
  mimeType?: string
}
export interface ShareLocation {
  token: string
  apiBase: string
}
