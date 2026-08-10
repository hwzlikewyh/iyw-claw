"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"
import { AlertCircle, Loader2, PackageOpen, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { TaskArtifactFileRow } from "@/components/layout/task-artifact-file-row"
import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useTabStore } from "@/contexts/tab-context"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { listTaskArtifacts, type TaskArtifactInfo } from "@/lib/api"
import { onTransportReconnect, subscribe } from "@/lib/platform"
import { cn } from "@/lib/utils"

type Scope = "current" | "all"
const REFRESH_DEBOUNCE_MS = 80

interface TaskArtifactsTabProps {
  conversationId?: number
}

export function TaskArtifactsTab({
  conversationId: conversationIdOverride,
}: TaskArtifactsTabProps = {}) {
  const t = useTranslations("Folder.taskArtifacts")
  const { activeFolderId } = useActiveFolder()
  const tabs = useTabStore((state) => state.tabs)
  const activeTabId = useTabStore((state) => state.activeTabId)
  const activeConversationId = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId)?.conversationId ?? null,
    [activeTabId, tabs]
  )
  const conversationId = conversationIdOverride ?? activeConversationId
  const [scope, setScope] = useState<Scope>("current")
  const effectiveScope =
    scope === "current" && conversationId == null ? "all" : scope
  const [items, setItems] = useState<TaskArtifactInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(false)
  const [selected, setSelected] = useState<TaskArtifactInfo | null>(null)
  const requestIdRef = useRef(0)
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const load = useCallback(async () => {
    const requestId = ++requestIdRef.current
    setLoading(true)
    setError(false)
    try {
      const next =
        effectiveScope === "all" && activeFolderId == null
          ? []
          : await listTaskArtifacts(
              effectiveScope === "current"
                ? { conversationId }
                : { folderId: activeFolderId }
            )
      if (requestId !== requestIdRef.current) return
      setItems(next)
      setSelected((current) =>
        current ? (next.find((item) => item.id === current.id) ?? null) : null
      )
    } catch {
      if (requestId !== requestIdRef.current) return
      setError(true)
    } finally {
      if (requestId === requestIdRef.current) setLoading(false)
    }
  }, [activeFolderId, conversationId, effectiveScope])

  const scheduleLoad = useCallback(() => {
    if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current)
    refreshTimerRef.current = setTimeout(() => {
      refreshTimerRef.current = null
      void load()
    }, REFRESH_DEBOUNCE_MS)
  }, [load])

  useEffect(() => {
    void load()
    return () => {
      requestIdRef.current += 1
    }
  }, [load])
  useEffect(() => {
    let disposed = false
    let unsubscribe: (() => void) | undefined
    void subscribe<{ conversationId: number }>(
      "task-artifact://changed",
      scheduleLoad
    ).then((stop) => {
      if (disposed) stop()
      else unsubscribe = stop
    })
    const stopReconnect = onTransportReconnect(scheduleLoad)
    return () => {
      disposed = true
      unsubscribe?.()
      stopReconnect?.()
      if (refreshTimerRef.current) {
        clearTimeout(refreshTimerRef.current)
        refreshTimerRef.current = null
      }
    }
  }, [scheduleLoad])

  const groups = useMemo(() => {
    if (effectiveScope === "current")
      return [{ id: conversationId ?? 0, title: null, agentType: null, items }]
    const map = new Map<number, TaskArtifactInfo[]>()
    for (const item of items)
      map.set(item.conversationId, [
        ...(map.get(item.conversationId) ?? []),
        item,
      ])
    return Array.from(map, ([id, grouped]) => ({
      id,
      title: grouped[0]?.conversationTitle,
      agentType: grouped[0]?.agentType,
      items: grouped,
    }))
  }, [conversationId, effectiveScope, items])

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-11 items-center gap-1 border-b px-2">
        {(["current", "all"] as const).map((value) => (
          <button
            key={value}
            type="button"
            disabled={value === "current" && conversationId == null}
            onClick={() => setScope(value)}
            className={cn(
              "h-7 flex-1 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-muted/70",
              effectiveScope === value &&
                "bg-background text-foreground shadow-xs",
              "disabled:pointer-events-none disabled:opacity-40"
            )}
          >
            {t(value)}
          </button>
        ))}
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={() => void load()}
          aria-label={t("refresh")}
          title={t("refresh")}
        >
          <RefreshCw className="size-3.5" />
        </Button>
      </div>
      {loading ? (
        <State
          icon={<Loader2 className="size-5 animate-spin" />}
          text={t("loading")}
        />
      ) : error ? (
        <State
          icon={<AlertCircle className="size-5" />}
          text={t("loadFailed")}
          action={t("retry")}
          onAction={() => void load()}
        />
      ) : items.length === 0 ? (
        <State icon={<PackageOpen className="size-6" />} text={t("empty")} />
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <div className="space-y-3 p-2">
            {groups.map((group) => (
              <section key={group.id} className="space-y-1">
                {group.title !== null && (
                  <div className="flex min-w-0 items-center justify-between gap-2 px-1 text-xs text-muted-foreground">
                    <span className="min-w-0 truncate">
                      {group.title || t("untitled")}
                    </span>
                    <span className="shrink-0">
                      {group.agentType
                        ? getAgentDisplayName(group.agentType)
                        : null}
                      {group.agentType ? " · " : null}
                      {group.items.length}
                    </span>
                  </div>
                )}
                {group.items.map((item) => (
                  <TaskArtifactFileRow
                    key={item.id}
                    item={item}
                    onView={setSelected}
                  />
                ))}
              </section>
            ))}
          </div>
        </ScrollArea>
      )}
      <TaskArtifactDialog
        artifact={selected}
        open={selected !== null}
        onOpenChange={(open) => !open && setSelected(null)}
      />
    </div>
  )
}

function State({
  icon,
  text,
  action,
  onAction,
}: {
  icon: ReactNode
  text: string
  action?: string
  onAction?: () => void
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center text-muted-foreground">
      {icon}
      <p className="text-sm">{text}</p>
      {action && (
        <Button size="sm" variant="outline" onClick={onAction}>
          {action}
        </Button>
      )}
    </div>
  )
}
