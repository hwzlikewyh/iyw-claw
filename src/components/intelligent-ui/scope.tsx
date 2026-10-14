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
import { useUiActions } from "./actions"

export interface UiScope {
  conversationId: number
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
  return conversationId ? (
    <IntelligentUiScope
      key={conversationId}
      conversationId={conversationId}
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
  useEffect(() => retainUiStateEvents(conversationId), [conversationId])
  return (
    <Scope.Provider
      value={{ conversationId, streaming, enabled, closedSources, ask }}
    >
      {children}
    </Scope.Provider>
  )
}
