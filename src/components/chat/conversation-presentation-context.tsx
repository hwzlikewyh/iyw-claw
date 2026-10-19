"use client"

import { createContext, useContext, useMemo, type ReactNode } from "react"
import { useRuntimeTasks } from "@/hooks/use-runtime-tasks"
import type { ConnectionStatus } from "@/lib/types"

type TaskState = ReturnType<typeof useRuntimeTasks>
interface PresentationState extends TaskState {
  contextKey?: string
  connectionId: string | null
  conversationId: number | null
  status: ConnectionStatus | null
  awaitingInput: boolean
}
const PresentationContext = createContext<PresentationState | null>(null)

export function ConversationPresentationProvider({
  children,
  contextKey,
  connectionId = null,
  conversationId = null,
  status,
  awaitingInput,
}: {
  children: ReactNode
  contextKey?: string
  connectionId?: string | null
  conversationId?: number | null
  status: ConnectionStatus | null
  awaitingInput: boolean
}) {
  const persistedId =
    conversationId != null && conversationId > 0 ? conversationId : null
  const state = useRuntimeTasks(connectionId, persistedId)
  const value = useMemo(
    () => ({
      ...state,
      contextKey,
      connectionId,
      conversationId,
      status,
      awaitingInput,
    }),
    [state, contextKey, connectionId, conversationId, status, awaitingInput]
  )
  return (
    <PresentationContext.Provider value={value}>
      {children}
    </PresentationContext.Provider>
  )
}

export function useConversationPresentation() {
  return useContext(PresentationContext)
}
