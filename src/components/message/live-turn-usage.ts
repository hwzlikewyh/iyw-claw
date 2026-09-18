import { getFolderConversation } from "@/lib/api"
import type { DbConversationDetail, TurnUsage } from "@/lib/types"
import { addUsagePoints } from "@/lib/usage-points"

function mergeUsage(total: TurnUsage | null, usage: TurnUsage): TurnUsage {
  if (!total) return { ...usage }
  return {
    input_tokens: total.input_tokens + usage.input_tokens,
    output_tokens: total.output_tokens + usage.output_tokens,
    cache_read_input_tokens:
      total.cache_read_input_tokens + usage.cache_read_input_tokens,
    cache_creation_input_tokens:
      total.cache_creation_input_tokens + usage.cache_creation_input_tokens,
    estimated_points: addUsagePoints(total, usage),
  }
}

function sameUsageSnapshot(
  first: DbConversationDetail,
  page: DbConversationDetail
) {
  const a = first.session_stats?.total_usage
  const b = page.session_stats?.total_usage
  return (
    first.summary.external_id === page.summary.external_id &&
    first.in_flight_user_turn_id === page.in_flight_user_turn_id &&
    first.history_total_turns === page.history_total_turns &&
    a?.input_tokens === b?.input_tokens &&
    a?.output_tokens === b?.output_tokens &&
    a?.cache_read_input_tokens === b?.cache_read_input_tokens &&
    a?.cache_creation_input_tokens === b?.cache_creation_input_tokens
  )
}

export async function loadLiveTurnUsage(
  conversationId: number,
  isCurrent: () => boolean
): Promise<TurnUsage | null> {
  const first = await getFolderConversation(conversationId, undefined, true)
  const boundary = first.in_flight_user_turn_id
  if (!boundary || !isCurrent()) return null
  let page = first
  let usage: TurnUsage | null = null
  // 使用后端关联的本轮用户消息作为边界，避免客户端时钟偏差和历史消耗串轮。
  while (isCurrent()) {
    for (let index = page.turns.length - 1; index >= 0; index -= 1) {
      const turn = page.turns[index]
      if (turn.id === boundary) return usage
      if (turn.role === "assistant" && turn.usage) {
        usage = mergeUsage(usage, turn.usage)
      }
    }
    if (page.history_start === 0) return null
    const before = page.history_start
    page = await getFolderConversation(conversationId, before, true)
    if (page.history_start >= before || !sameUsageSnapshot(first, page))
      return null
  }
  return null
}
