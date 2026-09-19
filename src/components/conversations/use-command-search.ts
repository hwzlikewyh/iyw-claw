"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useIsActiveChatMode } from "@/hooks/use-is-active-chat-mode"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { listTaskArtifacts } from "@/lib/api"
import {
  listConversationsPage,
  type ConversationCursor,
} from "@/lib/conversation-pages"
import { extractAppCommandError } from "@/lib/app-error"
import { loadReferenceFiles } from "@/lib/reference-file-loader"
import { compareAgentType, type AgentType } from "@/lib/types"

const SEARCH_DEBOUNCE_MS = 300
export const SEARCH_PAGE_SIZE = 100

export function useCommandSearchScope() {
  const { activeFolder } = useActiveFolder()
  const isChat = useIsActiveChatMode()
  const conversations = useAppWorkspaceStore((state) => state.conversations)
  const folder = isChat ? null : activeFolder
  const folderId = folder?.id ?? null
  const agents = useMemo(
    () =>
      Array.from(
        new Set(
          conversations
            .filter((item) => folderId === null || item.folder_id === folderId)
            .map((item) => item.agent_type)
        )
      ).sort(compareAgentType),
    [conversations, folderId]
  )
  return { folder, folderId, agents }
}

interface SearchRequest<T> {
  key: string
  source: string
  load: (signal: AbortSignal) => Promise<T>
  enabled?: boolean
  delay?: number
}

function useSearchRequest<T>({
  key,
  source,
  load,
  enabled = true,
  delay = SEARCH_DEBOUNCE_MS,
}: SearchRequest<T>) {
  const [attempt, setAttempt] = useState(0)
  const requestKey = `${key}:${attempt}`
  const [state, setState] = useState<{
    key: string
    data: T | null
    error: boolean
  } | null>(null)
  useEffect(() => {
    if (!enabled) return
    const controller = new AbortController()
    const timer = setTimeout(() => {
      load(controller.signal).then(
        (data) => {
          if (!controller.signal.aborted)
            setState({ key: requestKey, data, error: false })
        },
        (error: unknown) => {
          if (controller.signal.aborted) return
          console.error("[command-search] request failed", {
            source,
            code: extractAppCommandError(error)?.code,
            errorType: error instanceof Error ? error.name : typeof error,
          })
          setState({ key: requestKey, data: null, error: true })
        }
      )
    }, delay)
    return () => {
      clearTimeout(timer)
      controller.abort()
    }
  }, [delay, enabled, load, requestKey, source])
  const current = enabled && state?.key === requestKey ? state : null
  return {
    data: current?.data ?? null,
    loading: enabled && current === null,
    error: current?.error ?? false,
    retry: () => setAttempt((value) => value + 1),
  }
}

export function useConversationSearch(filters: {
  query: string
  folderId: number | null
  agent: AgentType | null
  cursor: ConversationCursor | null
}) {
  const { query, folderId, agent, cursor } = filters
  const search = query.trim()
  const load = useCallback(
    () =>
      listConversationsPage({
        folderIds: folderId === null ? null : [folderId],
        agentType: agent,
        search: search || null,
        cursor,
        pageSize: SEARCH_PAGE_SIZE,
      }),
    [folderId, agent, search, cursor]
  )
  return useSearchRequest({
    key: JSON.stringify([folderId, agent, search, cursor]),
    source: "conversations",
    load,
    enabled: !!search || !!agent,
  })
}

export function useArtifactSearch(filters: {
  query: string
  folderId: number | null
  page: number
}) {
  const { query, folderId, page } = filters
  const search = query.trim()
  const load = useCallback(
    () =>
      listTaskArtifacts({ folderId, search, page, pageSize: SEARCH_PAGE_SIZE }),
    [folderId, search, page]
  )
  return useSearchRequest({
    key: JSON.stringify([folderId, search, page]),
    source: "artifacts",
    load,
  })
}

export function useSearchFiles(folderPath: string | undefined) {
  const load = useCallback(
    (signal: AbortSignal) => loadReferenceFiles(folderPath!, signal),
    [folderPath]
  )
  return useSearchRequest({
    key: folderPath ?? "",
    source: "files",
    load,
    enabled: !!folderPath,
    delay: 0,
  })
}
