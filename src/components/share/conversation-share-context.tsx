"use client"

import {
  createContext,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import type { AgentType } from "@/lib/types"
import { useShareComposer } from "./use-share-composer"

type Composer = ReturnType<typeof useShareComposer>
interface ShareContextValue {
  state: Composer | null
  conversationId: number
  start: () => void
  close: () => void
}

const ShareContext = createContext<ShareContextValue | null>(null)

export const useConversationShare = () => useContext(ShareContext)

export function ConversationShareProvider({
  conversationId,
  agentType,
  children,
}: {
  conversationId?: number | null
  agentType?: AgentType | null
  children: ReactNode
}) {
  const [session, setSession] = useState<{
    id: number
    agent: AgentType
  } | null>(null)
  const active =
    session !== null &&
    session.id === conversationId &&
    session.agent === agentType
  const state = useShareComposer(
    conversationId ?? 0,
    agentType ?? "codex",
    active
  )
  const activeState = active ? state : null
  const reset = state.reset
  const value = useMemo(
    () =>
      conversationId && conversationId > 0 && agentType
        ? {
            state: activeState,
            conversationId,
            start: () => {
              reset()
              setSession({ id: conversationId, agent: agentType })
            },
            close: () => {
              if (activeState?.status) return
              setSession(null)
              reset()
            },
          }
        : null,
    [conversationId, agentType, activeState, reset]
  )
  return <ShareContext.Provider value={value}>{children}</ShareContext.Provider>
}
