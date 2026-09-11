export interface SideQuestionRequest {
  sessionId: string
  action: "capabilities" | "ask" | "cancel"
  requestId?: string
  question?: string
}

export interface SideQuestionResult {
  supported?: boolean
  mode?: "native-context-only" | "native-read-only"
  response?: string
  cancelled?: boolean
  synthetic?: boolean
}

export interface SideQuestionEntry {
  id: string
  question: string
  answer?: string
  error?: string
  status: "running" | "completed" | "cancelled" | "failed"
}

/** Exact leading command, not /btwhatever or an occurrence inside quoted prose. */
export function parseSideQuestion(text: string): string | null {
  const match = text.trimStart().match(/^\/btw(?:\s+([\s\S]*))?$/i)
  return match ? (match[1] ?? "").trim() : null
}
