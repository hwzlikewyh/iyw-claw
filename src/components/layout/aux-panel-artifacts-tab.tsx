"use client"

import { useCallback, useMemo, useState, type ReactNode } from "react"
import {
  AlertCircle,
  ChevronLeft,
  ChevronRight,
  Loader2,
  PackageOpen,
  RefreshCw,
  Search,
} from "lucide-react"
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
import { Input } from "@/components/ui/input"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useTabStore } from "@/contexts/tab-context"
import type { TaskArtifactInfo } from "@/lib/api"
import { cn } from "@/lib/utils"

type Scope = "current" | "all"
type DisplayMode = "panel" | "browser"

interface TaskArtifactsTabProps {
  /** `null` explicitly means the current draft is not persisted yet. */
  conversationId?: number | null
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
  const [search, setSearch] = useState("")
  const [page, setPage] = useState(1)
  const effectiveScope = scope
  const filters = useMemo(
    () => ({
      conversationId,
      folderId: activeFolderId,
      scope: effectiveScope,
      search,
      page,
      pageSize: 50,
    }),
    [activeFolderId, conversationId, effectiveScope, page, search]
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
    setPage(1)
  }
  const changeSearch = (value: string) => {
    setSearch(value)
    setPage(1)
    selection.showList()
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <ArtifactToolbar
        scope={effectiveScope}
        conversationId={conversationId}
        loading={query.loading}
        refreshing={query.refreshing}
        onScopeChange={changeScope}
        search={search}
        onSearchChange={changeSearch}
        onRefresh={() => void query.refresh()}
      />
      <ArtifactContent
        {...query}
        groups={groups}
        scope={effectiveScope}
        search={search}
        displayMode={displayMode}
        selection={selection}
        onPageChange={setPage}
        onRequestClose={onRequestClose}
      />
      {displayMode === "panel" && <PanelArtifactDialog selection={selection} />}
    </div>
  )
}

function useConversationId(override?: number | null): number | null {
  const tabs = useTabStore((state) => state.tabs)
  const activeTabId = useTabStore((state) => state.activeTabId)
  return useMemo(() => {
    if (override !== undefined) return override
    return tabs.find((tab) => tab.id === activeTabId)?.conversationId ?? null
  }, [activeTabId, override, tabs])
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
  search,
  onSearchChange,
  onRefresh,
}: {
  scope: Scope
  conversationId: number | null
  loading: boolean
  refreshing: boolean
  onScopeChange: (scope: Scope) => void
  search: string
  onSearchChange: (value: string) => void
  onRefresh: () => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="flex min-h-11 shrink-0 flex-wrap items-center gap-1 border-b px-2 py-2">
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
      <div className="relative order-3 basis-full sm:order-none sm:flex-1">
        <Search className="pointer-events-none absolute start-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder={t("searchPlaceholder")}
          aria-label={t("searchLabel")}
          className="h-7 rounded-md py-0 ps-7 pe-2 text-xs"
        />
      </div>
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
  total: number
  page: number
  pageSize: number
  loading: boolean
  error: boolean
  refresh: () => Promise<void>
  groups: TaskArtifactGroup[]
  scope: Scope
  search: string
  displayMode: DisplayMode
  selection: ArtifactSelection
  onPageChange: (page: number) => void
  onRequestClose?: () => void
}

function ArtifactContent({
  items,
  total,
  page,
  pageSize,
  loading,
  error,
  refresh,
  groups,
  scope,
  search,
  displayMode,
  selection,
  onPageChange,
  onRequestClose,
}: ArtifactContentProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const hasSearch = search.trim().length > 0
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
        text={t(
          scope === "current"
            ? hasSearch
              ? "emptySearch"
              : "emptyCurrent"
            : hasSearch
              ? "emptySearch"
              : "emptyAll"
        )}
      />
    )
  }
  if (displayMode === "panel") {
    return (
      <ArtifactResults
        total={total}
        page={page}
        pageSize={pageSize}
        onPageChange={onPageChange}
      >
        <TaskArtifactsList groups={groups} onSelect={selection.select} />
      </ArtifactResults>
    )
  }
  return (
    <ArtifactResults
      total={total}
      page={page}
      pageSize={pageSize}
      onPageChange={onPageChange}
    >
      <TaskArtifactsBrowser
        groups={groups}
        selection={selection}
        onRequestClose={onRequestClose}
      />
    </ArtifactResults>
  )
}

function ArtifactResults({
  children,
  total,
  page,
  pageSize,
  onPageChange,
}: {
  children: ReactNode
  total: number
  page: number
  pageSize: number
  onPageChange: (page: number) => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
      <ArtifactPagination
        total={total}
        page={page}
        pageSize={pageSize}
        onPageChange={onPageChange}
      />
    </div>
  )
}

function ArtifactPagination({
  total,
  page,
  pageSize,
  onPageChange,
}: {
  total: number
  page: number
  pageSize: number
  onPageChange: (page: number) => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  const totalPages = Math.max(1, Math.ceil(total / Math.max(1, pageSize)))
  if (total <= 0) return null
  const currentPage = Math.min(Math.max(1, page), totalPages)
  const start = (currentPage - 1) * pageSize + 1
  const end = Math.min(currentPage * pageSize, total)
  return (
    <nav
      className="flex shrink-0 items-center justify-between gap-2 border-t px-3 py-2"
      aria-label={t("paginationLabel")}
    >
      <span className="truncate text-[11px] text-muted-foreground">
        {t("paginationSummary", { start, end, total })}
      </span>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          type="button"
          size="icon-xs"
          variant="ghost"
          disabled={currentPage <= 1}
          onClick={() => onPageChange(currentPage - 1)}
          aria-label={t("paginationPrevious")}
          title={t("paginationPrevious")}
        >
          <ChevronLeft className="size-3.5" aria-hidden="true" />
        </Button>
        <span className="min-w-14 text-center text-[11px] text-muted-foreground">
          {t("paginationPage", { page: currentPage, pages: totalPages })}
        </span>
        <Button
          type="button"
          size="icon-xs"
          variant="ghost"
          disabled={currentPage >= totalPages}
          onClick={() => onPageChange(currentPage + 1)}
          aria-label={t("paginationNext")}
          title={t("paginationNext")}
        >
          <ChevronRight className="size-3.5" aria-hidden="true" />
        </Button>
      </div>
    </nav>
  )
}
