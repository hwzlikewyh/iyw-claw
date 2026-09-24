"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import {
  listConversationsPage,
  type ConversationCursor,
} from "@/lib/conversation-pages"
import { getTransport } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { resolveConversationFolderScope } from "./conversation-folder-scope"
import type {
  AgentType,
  ConversationStatus,
  DbConversationSummary,
} from "@/lib/types"

import {
  runConversationBatch,
  type ManagerAction,
} from "./conversation-manager-actions"

const SEARCH_DELAY_MS = 250
const PAGE_SIZE = 100

export function useConversationManager({
  open,
  folderId,
}: {
  open: boolean
  folderId?: number
}) {
  const t = useTranslations("SidebarDesign")
  const folders = useAppWorkspaceStore((s) => s.allFolders)
  const live = useAppWorkspaceStore((s) => s.conversations)
  const { closeConversationTab, openTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const [search, setSearch] = useState("")
  const [project, setProject] = useState("all")
  const [agent, setAgent] = useState<AgentType | "all">("all")
  const [status, setStatus] = useState<ConversationStatus | "all">("all")
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [rows, setRows] = useState<DbConversationSummary[]>([])
  const [cursor, setCursor] = useState<ConversationCursor | null>(null)
  const [loading, setLoading] = useState(false)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [incomplete, setIncomplete] = useState(false)
  const [loadedKey, setLoadedKey] = useState("")
  const [reload, setReload] = useState(0)
  const generation = useRef(0)
  const busy = useRef(false)
  const navigating = useRef(false)
  const activeBatch = useRef<AbortController | null>(null)
  useEffect(() => () => activeBatch.current?.abort(), [])
  const pageBusy = useRef(false)
  useEffect(
    () =>
      useAppWorkspaceStore.subscribe((current, previous) => {
        if (current.conversations === previous.conversations) return
        const present = new Set(current.conversations.map((row) => row.id))
        const removed = new Set(
          previous.conversations
            .filter((row) => !present.has(row.id))
            .map((row) => row.id)
        )
        if (removed.size)
          setRows((value) => value.filter((row) => !removed.has(row.id)))
      }),
    []
  )
  const scope = useMemo(() => {
    const id = folderId ?? (project === "all" ? undefined : Number(project))
    return id === undefined ? null : resolveConversationFolderScope(id, folders)
  }, [folderId, folders, project])
  const key = JSON.stringify([scope, search.trim(), agent, status, reload])
  const fetchPage = useCallback(
    async (next: ConversationCursor | null, request: number) => {
      if (pageBusy.current && next) return
      pageBusy.current = true
      setLoading(true)
      setError(null)
      try {
        const page = await listConversationsPage({
          folderIds: scope,
          search: search.trim() || null,
          agentType: agent === "all" ? null : agent,
          status: status === "all" ? null : status,
          sortBy: "newest",
          pageSize: PAGE_SIZE,
          cursor: next,
        })
        if (generation.current !== request) return
        setRows((prev) =>
          next
            ? [
                ...new Map(
                  [...prev, ...page.items].map((row) => [row.id, row])
                ).values(),
              ]
            : page.items
        )
        setCursor(page.next_cursor)
        setIncomplete(!!page.incomplete)
        setLoadedKey(key)
      } catch (reason) {
        if (generation.current !== request) return
        setError(toErrorMessage(reason))
        setLoadedKey(key)
        if (!next) {
          setRows([])
          setCursor(null)
        }
        console.error("[conversation-manager] query failed", {
          message: toErrorMessage(reason),
        })
      } finally {
        if (generation.current === request) {
          pageBusy.current = false
          setLoading(false)
        }
      }
    },
    [scope, search, agent, status, key]
  )
  useEffect(() => {
    if (!open) return
    const request = ++generation.current
    const invalidate = () => {
      generation.current = request + 1
    }
    const timer = window.setTimeout(
      () => void fetchPage(null, request),
      SEARCH_DELAY_MS
    )
    return () => {
      window.clearTimeout(timer)
      invalidate()
    }
  }, [open, fetchPage])
  const visible = useMemo(() => {
    if (loadedKey !== key) return []
    const latest = new Map(live.map((row) => [row.id, row]))
    return rows
      .map((row) => {
        const current = latest.get(row.id)
        return current &&
          Date.parse(current.updated_at) >= Date.parse(row.updated_at)
          ? current
          : row
      })
      .filter((row) => status === "all" || row.status === status)
  }, [rows, live, loadedKey, key, status])
  const select = (id: number) =>
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  const selectedRows = visible.filter((row) => selected.has(row.id))
  const selectAll = () =>
    setSelected(
      selectedRows.length === visible.length
        ? new Set()
        : new Set(visible.map((row) => row.id))
    )
  const run = async (action: ManagerAction, targets = selectedRows) => {
    if (busy.current || !targets.length) return
    busy.current = true
    setPending(true)
    const backend = getTransport()
    const controller = new AbortController()
    activeBatch.current = controller
    try {
      const results = await runConversationBatch({
        action,
        rows: targets,
        signal: controller.signal,
        onDeleted: (row) =>
          closeConversationTab(row.folder_id, row.id, row.agent_type),
      })
      if (!results) return
      const failed = results.filter((result) => result.error)
      const success = results.length - failed.length
      console.info("[conversation-manager] batch finished", {
        action,
        success,
        failed: failed.length,
      })
      if (failed.length) {
        console.error("[conversation-manager] batch failures", failed)
        toast.error(
          t("partialFailure", {
            success,
            failed: failed.length,
            message: failed[0].error!,
          })
        )
      } else if (success) toast.success(t("operationDone", { count: success }))
      setSelected(new Set(failed.map((result) => result.id)))
      if (backend === getTransport()) {
        await useAppWorkspaceStore.getState().refreshConversations()
        setReload((value) => value + 1)
      }
    } finally {
      if (!controller.signal.aborted) setPending(false)
      activeBatch.current = null
      busy.current = false
    }
  }
  const navigate = async (row: DbConversationSummary) => {
    if (navigating.current || busy.current) return false
    navigating.current = true
    const backend = getTransport()
    try {
      const workspace = useAppWorkspaceStore.getState()
      if (
        row.kind !== "chat" &&
        !workspace.folders.some((folder) => folder.id === row.folder_id)
      ) {
        await workspace.addFolderToWorkspaceById(row.folder_id)
      }
      if (backend !== getTransport()) return false
      openConversations()
      openTab(
        row.folder_id,
        row.id,
        row.agent_type,
        true,
        row.title ?? undefined
      )
      return true
    } catch (reason) {
      toast.error(t("actionFailed", { message: toErrorMessage(reason) }))
      return false
    } finally {
      navigating.current = false
    }
  }
  return {
    search,
    setSearch: (value: string) => {
      setSearch(value)
      setSelected(new Set())
    },
    project,
    setProject: (value: string) => {
      setProject(value)
      setSelected(new Set())
    },
    agent,
    setAgent: (value: AgentType | "all") => {
      setAgent(value)
      setSelected(new Set())
    },
    status,
    setStatus: (value: ConversationStatus | "all") => {
      setStatus(value)
      setSelected(new Set())
    },
    selected,
    select,
    selectAll,
    selectedRows,
    rows: visible,
    loading: loading || (open && loadedKey !== key),
    pending,
    error,
    incomplete,
    folders,
    cursor,
    loadMore: () => void fetchPage(cursor, generation.current),
    refresh: () => setReload((value) => value + 1),
    run,
    navigate,
  }
}

export type ConversationManager = ReturnType<typeof useConversationManager>
