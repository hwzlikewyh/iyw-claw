"use client"
import { createContext, useContext, type ReactNode } from "react"
const Context = createContext<
  ((text: string) => void | Promise<void>) | undefined
>(undefined)
const Conversation = createContext<{
  runtimeId: number
  persistedId: number | null
} | null>(null)
export function UiActionsProvider({
  ask,
  conversationId,
  persistedConversationId,
  children,
}: {
  ask?: (text: string) => void | Promise<void>
  conversationId: number
  persistedConversationId: number | null
  children: ReactNode
}) {
  const persistedId =
    persistedConversationId !== null && persistedConversationId > 0
      ? persistedConversationId
      : null
  return (
    <Conversation.Provider value={{ runtimeId: conversationId, persistedId }}>
      <Context.Provider value={ask}>{children}</Context.Provider>
    </Conversation.Provider>
  )
}
export const useUiActions = () => useContext(Context)
export const useUiConversation = () => useContext(Conversation)
