import { classifyCollabStatus, type CollabStatusKind } from "./collab-tool"
import type { ConversationDetail } from "./types"

export function collabActionDetail(input: string | null) {
  if (!input) return null
  try {
    const value = JSON.parse(input)
    const detail =
      value?.description ??
      value?.file_path ??
      value?.path ??
      value?.cmd ??
      value?.command
    return typeof detail === "string" ? detail : null
  } catch {
    return null
  }
}

export const COLLAB_STATUS_LABEL = {
  pending: "statusPending",
  running: "statusRunning",
  completed: "statusCompleted",
  closed: "statusClosed",
  interrupted: "statusInterrupted",
  failed: "statusFailed",
  notFound: "statusNotFound",
  other: "statusUnknown",
} as const satisfies Record<CollabStatusKind, string>

export function isCollabActive(status: string | null) {
  return ["pending", "running"].includes(classifyCollabStatus(status))
}

export function collabWaitLabel(
  output: string | null | undefined
): CollabWaitLabel {
  if (!output) return "waitFinished"
  try {
    const value = JSON.parse(output)
    if (value?.timed_out === true) return "waitTimedOut"
    if (typeof value?.message !== "string") return "waitFinished"
    if (value.message.startsWith("Wait interrupted")) return "statusInterrupted"
    if (value.message.startsWith("Wait completed")) return "waitReceived"
  } catch {
    // 旧版本没有结构化等待结果，只展示已确认的等待状态。
  }
  return "waitFinished"
}

type CollabWaitLabel =
  | "waitFinished"
  | "waitTimedOut"
  | "statusInterrupted"
  | "waitReceived"
  | "opWait"
  | "statusRunning"
  | "statusFailed"
  | "noMessage"

export function collabTranscriptSummary(detail: ConversationDetail | null) {
  const turns = detail?.turns ?? []
  const assistant = [...turns]
    .reverse()
    .find((turn) => turn.role === "assistant")
  const blocks = assistant?.blocks ?? []
  const lastAction = [...blocks]
    .reverse()
    .find((block) => block.type === "tool_use" || block.type === "text")
  const text = blocks
    .filter((block) => block.type === "text")
    .map((block) => block.text)
    .join("\n\n")
  return { lastAction, text }
}
