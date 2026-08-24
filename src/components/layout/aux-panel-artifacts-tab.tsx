"use client"

import { useCallback, useMemo, useState } from "react"
import { AlertCircle, Loader2, PackageOpen, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  TaskArtifactState,
  TaskArtifactsBrowser,
  type TaskArtifactsBrowserSelection,
} from "@/components/layout/task-artifacts-browser"
import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import {
  TaskArtifactsList,
  type TaskArtifactGroup,
} from "@/components/layout/task-artifacts-list"
import { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { Button } from "@/components/ui/button"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useTabStore } from "@/contexts/tab-context"
import type { TaskArtifactInfo } from "@/lib/api"
import { cn } from "@/lib/utils"

type Scope = "current" | "all"
type DisplayMode = "panel" | "browser"

interface TaskArtifactsTabProps {
  conversationId?: number
  displayMode?: DisplayMode
  onRequestClose?: () => void
}

export function TaskArtifactsTab({
  conversationId: conversationIdOverride,
  displayMode = "panel",
  onRequestClose,
}: TaskArtifactsTabProps = {}) {
  const { activeFolderId } = useActiveFolder()
  const conversationId = useConversationId(conversationIdOverride)
  const [scope, setScope] = useState<Scope>("current")
  const effectiveScope = scope
  const filters = useMemo(
    () => ({ conversationId, folderId: activeFolderId, scope: effectiveScope }),
    [activeFolderId, conversationId, effectiveScope]
  )
  const query = useTaskArtifacts(filters)
  const selection = useArtifactSelection(query.items, displayMode)
  const groups = useMemo(
    () => groupArtifacts(query.items, effectiveScope, conversationId),
    [conversationId, effectiveScope, query.items]
  )
  const changeScope = (next: Scope) => {
    selection.showList()
    setScope(next)
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ArtifactToolbar
        scope={effectiveScope}
        conversationId={conversationId}
        loading={query.loading}
        refreshing={query.refreshing}
        onScopeChange={changeScope}
        onRefresh={() => void query.refresh()}
      />
      <ArtifactContent
        {...query}
        groups={groups}
        scope={effectiveScope}
        displayMode={displayMode}
        selection={selection}
        onRequestClose={onRequestClose}
      />
      {displayMode === "panel" && <PanelArtifactDialog selection={selection} />}
    </div>
  )
}

function useConversationId(override?: number): number | null {
  const tabs = useTabStore((state) => state.tabs)
  const activeTabId = useTabStore((state) => state.activeTabId)
  return useMemo(
    () =>
      override ??
      tabs.find((tab) => tab.id === activeTabId)?.conversationId ??
      null,
    [activeTabId, override, tabs]
  )
}

interface ArtifactSelection extends TaskArtifactsBrowserSelection {
  dialogArtifact: TaskArtifactInfo | null
  closeDialog: () => void
}

function PanelArtifactDialog({ selection }: { selection: ArtifactSelection }) {
  return (
    <TaskArtifactDialog
      artifact={selection.dialogArtifact}
      open={selection.dialogArtifact !== null}
      onOpenChange={(open) => !open && selection.closeDialog()}
    />
  )
}

function useArtifactSelection(
  items: TaskArtifactInfo[],
  displayMode: DisplayMode
): ArtifactSelection {
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [dialogArtifactId, setDialogArtifactId] = useState<number | null>(null)
  const [mobilePreviewOpen, setMobilePreviewOpen] = useState(false)
  const selected = useMemo(
    () => items.find((item) => item.id === selectedId) ?? items[0] ?? null,
    [items, selectedId]
  )
  const dialogArtifact = useMemo(
    () => items.find((item) => item.id === dialogArtifactId) ?? null,
    [dialogArtifactId, items]
  )
  const select = useCallback(
    (item: TaskArtifactInfo) => {
      if (displayMode === "panel") setDialogArtifactId(item.id)
      else {
        setSelectedId(item.id)
        setMobilePreviewOpen(true)
      }
    },
    [displayMode]
  )
  const showList = useCallback(() => setMobilePreviewOpen(false), [])
  const closeDialog = useCallback(() => setDialogArtifactId(null), [])
  return {
    selected,
    dialogArtifact,
    mobilePreviewOpen,
    select,
    showList,
    closeDialog,
  }
}

function groupArtifacts(
  items: TaskArtifactInfo[],
  scope: Scope,
  conversationId: number | null
): TaskArtifactGroup[] {
  if (scope === "current") {
    return [{ id: conversationId ?? 0, title: null, agentType: null, items }]
  }
  const grouped = new Map<number, TaskArtifactInfo[]>()
  for (const item of items) {
    const group = grouped.get(item.conversationId)
    if (group) group.push(item)
    else grouped.set(item.conversationId, [item])
  }
  return Array.from(grouped, ([id, group]) => ({
    id,
    title: group[0]?.conversationTitle ?? null,
    agentType: group[0]?.agentType ?? null,
    items: group,
  }))
}

function ArtifactToolbar({
  scope,
  conversationId,
  loading,
  refreshing,
  onScopeChange,
  onRefresh,
}: {
  scope: Scope
  conversationId: number | null
  loading: boolean
  refreshing: boolean
  onScopeChange: (scope: Scope) => void
  onRefresh: () => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="flex h-11 shrink-0 items-center gap-1 border-b px-2">
      {(["current", "all"] as const).map((value) => (
        <button
          key={value}
          type="button"
          disabled={value === "current" && conversationId == null}
          onClick={() => onScopeChange(value)}
          className={cn(
            "h-7 flex-1 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-muted/70",
            scope === value && "bg-background text-foreground shadow-xs",
            "disabled:pointer-events-none disabled:opacity-40"
          )}
        >
          {t(value)}
        </button>
      ))}
      <Button
        variant="ghost"
        size="icon-xs"
        disabled={loading || refreshing}
        onClick={onRefresh}
        aria-label={t("refresh")}
        title={t("refresh")}
      >
        <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} />
      </Button>
    </div>
  )
}

interface ArtifactContentProps {
  items: TaskArtifactInfo[]
  loading: boolean
  error: boolean
  refresh: () => Promise<void>
  groups: TaskArtifactGroup[]
  scope: Scope
  displayMode: DisplayMode
  selection: ArtifactSelection
  onRequestClose?: () => void
}

function ArtifactContent({
  items,
  loading,
  error,
  refresh,
  groups,
  scope,
  displayMode,
  selection,
  onRequestClose,
}: ArtifactContentProps) {
  const t = useTranslations("Folder.taskArtifacts")
  if (loading)
    return (
      <TaskArtifactState
        icon={<Loader2 className="size-5 animate-spin" />}
        text={t("loading")}
      />
    )
  if (error) {
    return (
      <TaskArtifactState
        icon={<AlertCircle className="size-5" />}
        text={t("loadFailed")}
        action={t("retry")}
        onAction={() => void refresh()}
      />
    )
  }
  if (items.length === 0) {
    return (
      <TaskArtifactState
        icon={<PackageOpen className="size-6" />}
        text={t(scope === "current" ? "emptyCurrent" : "emptyAll")}
      />
    )
  }
  if (displayMode === "panel") {
    return <TaskArtifactsList groups={groups} onSelect={selection.select} />
  }
  return (
    <TaskArtifactsBrowser
      groups={groups}
      selection={selection}
      onRequestClose={onRequestClose}
    />
  )
}
