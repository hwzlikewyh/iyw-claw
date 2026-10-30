"use client"

import { useEffect, type RefObject } from "react"
import type { MessageScrollPosition } from "@/components/message/virtualized-message-thread"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"

const HIDDEN_HISTORY_RETENTION_MS = 60_000

export function useInactiveConversationHistory(options: {
  conversationId: number
  visible: boolean
  scrollPositionRef: RefObject<MessageScrollPosition | null>
}): void {
  const { conversationId, visible, scrollPositionRef } = options
  useEffect(() => {
    if (visible) return
    const timer = setTimeout(() => {
      // 正在回看历史时保留原位置；只释放停在底部、已落盘的闲置会话。
      if (scrollPositionRef.current && !scrollPositionRef.current.wasAtBottom)
        return
      const store = useConversationRuntimeStore.getState()
      const session = store.byConversationId.get(conversationId)
      if (
        !session?.detail ||
        session.detailLoading ||
        session.detailError ||
        session.acpLoadError ||
        session.pendingCleanup ||
        session.syncState !== "idle" ||
        session.liveMessage ||
        session.localTurns.length ||
        session.optimisticTurns.length ||
        session.backgroundTurns.length ||
        session.pendingBackgroundSettlements.length
      )
        return
      store.actions.evictConversationContent(conversationId)
    }, HIDDEN_HISTORY_RETENTION_MS)
    return () => clearTimeout(timer)
  }, [conversationId, visible, scrollPositionRef])
}
