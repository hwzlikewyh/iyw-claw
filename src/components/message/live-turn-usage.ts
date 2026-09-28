import { subscribeConversationUsage } from "@/lib/conversation-usage-refresh"
import type { TurnUsage } from "@/lib/types"

export function subscribeLiveTurnUsage(
  conversationId: number,
  onUsage: (usage: TurnUsage) => void
) {
  return subscribeConversationUsage(
    conversationId,
    (snapshot) => {
      // 后台关联本轮用户消息，不比较不同机器的时钟，也不复用上一轮消费。
      if (snapshot.turnUsage && snapshot.inFlightUserTurnId != null)
        onUsage(snapshot.turnUsage)
    },
    true
  )
}
