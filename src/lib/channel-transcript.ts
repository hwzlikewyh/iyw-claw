import { denormalizeSnapshot } from "@/lib/snapshot-denormalize"
import { buildStreamingTurnsFromLiveMessage } from "@/stores/conversation-runtime-store"
import type {
  DbConversationDetail,
  LiveSessionSnapshot,
  MessageTurn,
} from "@/lib/types"

function textOf(turn: MessageTurn) {
  return turn.blocks
    .flatMap((block) => (block.type === "text" ? [block.text] : []))
    .join("")
    .trim()
}

function liveIsPersisted(history: MessageTurn[], live: MessageTurn[]) {
  const last = history[history.length - 1]
  const replies = live.filter((turn) => turn.role === "assistant")
  const reply = replies[replies.length - 1]
  return (
    !!last &&
    !!reply &&
    last.role === "assistant" &&
    (last.id === reply.id ||
      (Date.parse(last.timestamp) >= Date.parse(reply.timestamp) &&
        !!textOf(reply) &&
        textOf(last) === textOf(reply)))
  )
}

export function channelTranscript(data: {
  history: DbConversationDetail | null
  live: LiveSessionSnapshot | null
  older?: DbConversationDetail | null
}) {
  let history = [...(data.older?.turns ?? []), ...(data.history?.turns ?? [])]
  history = Array.from(new Map(history.map((turn) => [turn.id, turn])).values())
  const patch = data.live ? denormalizeSnapshot(data.live) : null
  const conversationId = data.history?.summary.id ?? data.live?.conversation_id
  if (!patch?.liveMessage || !conversationId)
    return { turns: history, streaming: new Set<number>() }
  const built = buildStreamingTurnsFromLiveMessage(
    -conversationId,
    patch.liveMessage
  )
  const streaming = data.live?.status === "prompting"
  if (!streaming && liveIsPersisted(history, built.turns))
    return { turns: history, streaming: new Set<number>() }
  const anchor = data.history?.in_flight_user_turn_id
  const boundary = anchor ? history.findIndex((turn) => turn.id === anchor) : -1
  if (streaming && boundary >= 0) history = history.slice(0, boundary + 1)
  const pending = data.live?.pending_user_message
  if (
    pending &&
    boundary < 0 &&
    !history.some((turn) => turn.id === pending.message_id)
  ) {
    history.push({
      id: pending.message_id,
      role: "user",
      blocks: pending.blocks,
      timestamp:
        data.live?.activity?.started_at ?? data.live!.live_message!.started_at,
    })
  }
  const ids = new Set(history.map((turn) => turn.id))
  const tail = built.turns.filter((turn) => !ids.has(turn.id))
  return {
    turns: [...history, ...tail],
    streaming: new Set(
      streaming ? tail.map((_, index) => history.length + index) : []
    ),
  }
}
