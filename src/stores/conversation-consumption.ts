import type { ConversationRuntimeSession } from "./conversation-runtime-store"
import type { MessageTurn, SessionStats, TurnUsage } from "@/lib/types"

let epoch = 0
export const consumptionEpoch = () => epoch
export const advanceConsumptionEpoch = () => {
  epoch += 1
}

function clearUsage(usage: TurnUsage): TurnUsage {
  return {
    ...usage,
    estimated_points: null,
    confirmed_points: { amount: null, state: "unavailable" },
  }
}
function clearTurn(turn: MessageTurn): MessageTurn {
  return turn.usage ? { ...turn, usage: clearUsage(turn.usage) } : turn
}
function clearStats(stats: SessionStats | null): SessionStats | null {
  return stats
    ? {
        ...stats,
        confirmed_consumption: { amount: null, state: "unavailable" },
        total_usage: stats.total_usage ? clearUsage(stats.total_usage) : null,
      }
    : null
}

export function clearSessionConsumption(
  session: ConversationRuntimeSession
): ConversationRuntimeSession {
  return {
    ...session,
    detailLoading: false,
    detail: session.detail
      ? {
          ...session.detail,
          backend_consumption: null,
          turns: session.detail.turns.map(clearTurn),
          session_stats: clearStats(session.detail.session_stats ?? null),
        }
      : null,
    sessionStats: clearStats(session.sessionStats),
    localTurns: session.localTurns.map(clearTurn),
    optimisticTurns: session.optimisticTurns.map(clearTurn),
    backgroundTurns: session.backgroundTurns.map((entry) => ({
      ...entry,
      turn: clearTurn(entry.turn),
    })),
  }
}
