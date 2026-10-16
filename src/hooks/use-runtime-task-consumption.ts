"use client"

import { useEffect, useReducer } from "react"
import { subscribeConversationUsage } from "@/lib/conversation-usage-refresh"
import type { DbConversationDetail } from "@/lib/types"

export type RuntimeConsumption = NonNullable<
  DbConversationDetail["backend_consumption"]
>
type State = { id: number | null; data: RuntimeConsumption | null }
const INITIAL: State = { id: null, data: null }
const replace = (_state: State, next: State) => next

export function useRuntimeTaskConsumption(conversationId: number | null) {
  const [state, dispatch] = useReducer(replace, INITIAL)
  useEffect(() => {
    if (conversationId == null) return
    return subscribeConversationUsage(
      conversationId,
      (snapshot) => {
        dispatch({
          id: conversationId,
          data: snapshot.backendConsumption ?? null,
        })
      },
      false
    )
  }, [conversationId])
  return state.id === conversationId ? state.data : null
}
