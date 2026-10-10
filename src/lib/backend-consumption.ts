import type { DbConversationDetail, TurnUsage } from "./types"

const EMPTY_USAGE: TurnUsage = {
  input_tokens: 0,
  output_tokens: 0,
  cache_read_input_tokens: 0,
  cache_creation_input_tokens: 0,
}

export function applyBackendConsumption(
  detail: DbConversationDetail
): DbConversationDetail {
  const points = detail.backend_consumption
  if (!points) {
    return {
      ...detail,
      session_stats: {
        ...(detail.session_stats ?? {
          total_usage: null,
          total_duration_ms: 0,
        }),
        // 本次权威读取无法核对时，不能沿用上一次登录身份的金额。
        confirmed_consumption: { amount: null, state: "unavailable" },
      },
    }
  }
  return {
    ...detail,
    turns: detail.turns.map((turn) => {
      const confirmed = turn.fork_message_id
        ? points.turns[turn.fork_message_id]
        : undefined
      return confirmed
        ? {
            ...turn,
            usage: {
              ...(turn.usage ?? confirmed.usage ?? EMPTY_USAGE),
              confirmed_points: confirmed,
            },
          }
        : turn
    }),
    session_stats: {
      ...(detail.session_stats ?? { total_usage: null, total_duration_ms: 0 }),
      confirmed_consumption: points.session,
      total_usage: detail.session_stats?.total_usage
        ? {
            ...detail.session_stats.total_usage,
            confirmed_points: points.session,
          }
        : null,
    },
  }
}
