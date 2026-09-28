import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import { invalidateConversationUsage } from "./conversation-usage-refresh"
import type { EventEnvelope } from "./types"

const UPDATED_EVENTS = new Set<EventEnvelope["type"]>([
  "usage_update",
  "session_started",
  "conversation_linked",
  "user_message",
])
const SETTLED_EVENTS = new Set<EventEnvelope["type"]>([
  "turn_complete",
  "error",
])

function sdkRefresh(message: unknown) {
  if (message == null || typeof message !== "object" || !("type" in message))
    return null
  if (message.type === "result") return "settled"
  return message.type === "assistant" ? "updated" : null
}

function refreshKind(event: EventEnvelope): "updated" | "settled" | null {
  if (UPDATED_EVENTS.has(event.type)) return "updated"
  if (SETTLED_EVENTS.has(event.type)) return "settled"
  if (event.type === "tool_call" || event.type === "tool_call_update") {
    return event.status === "completed" || event.status === "failed"
      ? "updated"
      : null
  }
  return secondaryRefresh(event)
}

function secondaryRefresh(event: EventEnvelope): "updated" | "settled" | null {
  switch (event.type) {
    case "background_activity":
      if (event.settled?.length) return "settled"
      return event.turns?.length ? "updated" : null
    case "status_changed":
      return statusRefresh(event.status)
    case "claude_sdk_message":
      return sdkRefresh(event.message)
    default:
      return null
  }
}

function statusRefresh(status: string) {
  if (status === "prompting") return "updated"
  return status === "connected" || status === "disconnected" ? "settled" : null
}

function resolveConversation(
  runtimeId: number | undefined,
  externalId: string | null | undefined
) {
  const state = useConversationRuntimeStore.getState()
  const id =
    runtimeId ??
    (externalId ? state.conversationIdByExternalId.get(externalId) : undefined)
  return id == null
    ? null
    : (state.byConversationId.get(id)?.dbConversationId ?? id)
}

function resetsUsage(event: EventEnvelope) {
  return (
    event.type === "session_started" ||
    event.type === "user_message" ||
    (event.type === "status_changed" && event.status === "prompting")
  )
}

export function notifyConversationUsage(
  event: EventEnvelope,
  runtimeId: number | undefined,
  externalId: string | null | undefined
) {
  const kind = refreshKind(event)
  if (!kind) return
  const conversationId =
    event.type === "conversation_linked"
      ? event.conversation_id
      : resolveConversation(runtimeId, externalId)
  if (conversationId != null && conversationId > 0) {
    invalidateConversationUsage(conversationId, {
      settled: kind === "settled",
      reset: resetsUsage(event),
    })
  }
}
