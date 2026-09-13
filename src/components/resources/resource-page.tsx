"use client"

import { useMemo, useState, type Dispatch, type SetStateAction } from "react"
import { LibraryBig, PackageCheck } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { ResourceResults } from "@/components/resources/resource-results"
import { ResourceToolbar } from "@/components/resources/resource-toolbar"
import type { ResourceSessionOption } from "@/components/resources/resource-toolbar"
import { ResourcePreview } from "@/components/resources/resource-preview"
import type { TaskArtifactInfo } from "@/lib/api"

const RESOURCE_PAGE_SIZE = 24

export function ResourcePage() {
  const model = useResourcePageModel()
  return <ResourcePageContent model={model} />
}

type ResourceFilters = {
  search: string
  session: string
  page: number
}

type ResourceQuery = ReturnType<typeof useTaskArtifacts>

interface ResourcePageModel {
  filters: ResourceFilters
  setFilters: Dispatch<SetStateAction<ResourceFilters>>
  view: "grid" | "list"
  setView: Dispatch<SetStateAction<"grid" | "list">>
  selected: TaskArtifactInfo | null
  setSelected: Dispatch<SetStateAction<TaskArtifactInfo | null>>
  query: ResourceQuery
  displayQuery: ResourceQuery
  sessionOptions: ResourceSessionOption[]
  filteredItems: TaskArtifactInfo[]
}

function useResourcePageModel(): ResourcePageModel {
  const [filters, setFilters] = useState({
    search: "",
    session: "all",
    page: 1,
  })
  const [view, setView] = useState<"grid" | "list">("grid")
  const [selected, setSelected] = useState<TaskArtifactInfo | null>(null)
  const query = useTaskArtifacts({
    search: filters.search,
    conversationId: null,
    folderId: null,
    scope: "all",
    page: 1,
    pageSize: 100,
    loadAll: true,
  })
  const sessionOptions = useSessionOptions(query.items)
  const filteredItems = useMemo(
    () =>
      filters.session === "all"
        ? query.items
        : query.items.filter(
            (item) => String(item.conversationId) === filters.session
          ),
    [filters.session, query.items]
  )
  const totalPages = Math.max(
    1,
    Math.ceil(filteredItems.length / RESOURCE_PAGE_SIZE)
  )
  const page = Math.min(filters.page, totalPages)
  const displayQuery = createDisplayQuery(query, filteredItems, page)
  const selection = selected
    ? (query.items.find((item) => item.id === selected.id) ?? selected)
    : null
  return {
    filters,
    setFilters,
    view,
    setView,
    selected: selection,
    setSelected,
    query,
    displayQuery,
    sessionOptions,
    filteredItems,
  }
}

function createDisplayQuery(
  query: ResourceQuery,
  filteredItems: TaskArtifactInfo[],
  page: number
): ResourceQuery {
  return {
    ...query,
    items: filteredItems.slice(
      (page - 1) * RESOURCE_PAGE_SIZE,
      page * RESOURCE_PAGE_SIZE
    ),
    total: filteredItems.length,
    page,
    pageSize: RESOURCE_PAGE_SIZE,
  }
}

function ResourcePageContent({ model }: { model: ResourcePageModel }) {
  const {
    filters,
    setFilters,
    view,
    setView,
    selected,
    setSelected,
    query,
    displayQuery,
    sessionOptions,
    filteredItems,
  } = model
  return (
    <section className="flex h-full min-h-0 min-w-0 flex-col bg-background">
      <ResourceHeader />
      <ResourcePageBody
        filters={filters}
        setFilters={setFilters}
        view={view}
        setView={setView}
        query={query}
        displayQuery={displayQuery}
        sessionOptions={sessionOptions}
        filteredItems={filteredItems}
        onSelect={setSelected}
      />
      <ResourcePreview artifact={selected} onClose={() => setSelected(null)} />
    </section>
  )
}

function ResourcePageBody({
  filters,
  setFilters,
  view,
  setView,
  query,
  displayQuery,
  sessionOptions,
  filteredItems,
  onSelect,
}: {
  filters: ResourceFilters
  setFilters: Dispatch<SetStateAction<ResourceFilters>>
  view: "grid" | "list"
  setView: Dispatch<SetStateAction<"grid" | "list">>
  query: ResourceQuery
  displayQuery: ResourceQuery
  sessionOptions: ResourceSessionOption[]
  filteredItems: TaskArtifactInfo[]
  onSelect: (item: TaskArtifactInfo) => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col px-4 sm:px-6 lg:px-8">
      <ResourceHeading
        total={filteredItems.length}
        sessionCount={
          new Set(filteredItems.map((item) => item.conversationId)).size
        }
        todayCount={filteredItems.filter(isToday).length}
        loading={query.loading}
        filtered={filters.session !== "all" || filters.search.trim() !== ""}
      />
      <ResourceToolbar
        search={filters.search}
        onSearchChange={(search) =>
          setFilters((value) => ({ ...value, search, page: 1 }))
        }
        session={filters.session}
        sessionOptions={sessionOptions}
        onSessionChange={(session) =>
          setFilters((value) => ({ ...value, session, page: 1 }))
        }
        view={view}
        onViewChange={setView}
        busy={query.loading || query.refreshing}
        onRefresh={() => void query.refresh()}
      />
      <ResourceResults
        query={displayQuery}
        search={filters.search}
        view={view}
        onSelect={onSelect}
        onClear={() => setFilters({ search: "", session: "all", page: 1 })}
        onPageChange={(page) => setFilters((value) => ({ ...value, page }))}
      />
    </div>
  )
}

function ResourceHeader() {
  const t = useTranslations("Resources")
  return (
    <header className="flex min-h-14 shrink-0 items-center gap-3 border-b px-4 sm:px-6">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-foreground text-background">
        <LibraryBig className="size-4" aria-hidden="true" />
      </span>
      <h1 className="text-sm font-semibold">{t("title")}</h1>
    </header>
  )
}

function ResourceHeading({
  total,
  sessionCount,
  todayCount,
  loading,
  filtered,
}: {
  total: number
  sessionCount: number
  todayCount: number
  loading: boolean
  filtered: boolean
}) {
  const t = useTranslations("Resources")
  return (
    <div className="flex flex-wrap items-start gap-x-5 gap-y-4 pt-7 pb-5">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 items-center gap-3">
          <h2 className="text-xl font-semibold">{t("deliverables")}</h2>
          {!loading && (
            <span className="rounded-md bg-muted px-2 py-0.5 text-xs tabular-nums text-muted-foreground">
              {t("count", { count: total })}
            </span>
          )}
        </div>
        <p className="text-xs text-muted-foreground">{t("subtitle")}</p>
      </div>
      <div className="flex shrink-0 items-start gap-5">
        <ResourceStat value={sessionCount} label={t("sessionCount")} />
        <ResourceStat value={todayCount} label={t("todayCount")} />
      </div>
      <span className="flex basis-full items-center gap-1.5 text-xs text-muted-foreground">
        <PackageCheck className="size-3.5" aria-hidden="true" />
        {filtered ? t("filteredResults") : t("allConversations")}
      </span>
    </div>
  )
}

function ResourceStat({ value, label }: { value: number; label: string }) {
  return (
    <span className="flex flex-col gap-0.5 text-xs text-muted-foreground">
      <strong className="text-sm font-semibold leading-5 text-foreground tabular-nums">
        {value}
      </strong>
      <span className="whitespace-nowrap">{label}</span>
    </span>
  )
}

function useSessionOptions(items: TaskArtifactInfo[]): ResourceSessionOption[] {
  return useMemo(
    () =>
      Array.from(
        new Map(
          items.map((item) => [
            item.conversationId,
            {
              id: item.conversationId,
              title: item.conversationTitle?.trim() || "",
            },
          ])
        ).values()
      ),
    [items]
  )
}

function isToday(item: TaskArtifactInfo): boolean {
  const date = new Date(item.createdAt)
  const today = new Date()
  return (
    !Number.isNaN(date.getTime()) &&
    date.getFullYear() === today.getFullYear() &&
    date.getMonth() === today.getMonth() &&
    date.getDate() === today.getDate()
  )
}
