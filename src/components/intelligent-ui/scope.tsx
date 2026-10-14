"use client"

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  type ReactNode,
} from "react"
import { retainUiStateEvents } from "@/lib/intelligent-ui/realtime"
import { useUiSettings } from "@/lib/intelligent-ui/settings"
import { scanUIBlockFences } from "@/lib/intelligent-ui/engine/fence"
import { useUiActions, useUiConversation } from "./actions"

export interface UiScope {
  conversationId: number
  persistedConversationId: number | null
  streaming: boolean
  enabled: boolean
  closedSources: ReadonlySet<string>
  ask?: (text: string) => void | Promise<void>
}
const Scope = createContext<UiScope | null>(null)
export const useUiScope = () => useContext(Scope)

export function AssistantUiText({
  conversationId,
  streaming,
  markdown,
  children,
}: {
  conversationId?: number
  streaming: boolean
  markdown: string
  children: ReactNode
}) {
  const ask = useUiActions()
  const conversation = useUiConversation()
  const persistedConversationId =
    conversation && conversation.runtimeId === conversationId
      ? conversation.persistedId
      : conversationId && conversationId > 0
        ? conversationId
        : null
  return conversationId ? (
    <IntelligentUiScope
      key={conversationId}
      conversationId={conversationId}
      persistedConversationId={persistedConversationId}
      streaming={streaming}
      markdown={markdown}
      ask={ask}
    >
      {children}
    </IntelligentUiScope>
  ) : (
    children
  )
}

function IntelligentUiScope({
  conversationId,
  persistedConversationId,
  streaming,
  ask,
  markdown,
  children,
}: Omit<UiScope, "enabled" | "closedSources"> & {
  markdown: string
  children: ReactNode
}) {
  const { enabled } = useUiSettings()
  const closedSources = useMemo(
    () =>
      new Set(
        scanUIBlockFences(markdown)
          .filter((fence) => fence.closed)
          .map((fence) => fence.body.trim())
      ),
    [markdown]
  )
  useEffect(
    () =>
      persistedConversationId !== null
        ? retainUiStateEvents(persistedConversationId)
        : undefined,
    [persistedConversationId]
  )
  return (
    <Scope.Provider
      value={{
        conversationId,
        persistedConversationId,
        streaming,
        enabled,
        closedSources,
        ask,
      }}
    >
      {children}
    </Scope.Provider>
  )
}
