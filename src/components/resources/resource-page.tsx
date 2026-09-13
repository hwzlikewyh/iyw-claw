"use client"

import {
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"
import { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { ResourceResults } from "@/components/resources/resource-results"
import { ResourceToolbar } from "@/components/resources/resource-toolbar"
import type { ResourceSessionOption } from "@/components/resources/resource-toolbar"
import { ResourcePreview } from "@/components/resources/resource-preview"
import {
  ResourceHeader,
  ResourceHeading,
} from "@/components/resources/resource-overview"
import type { TaskArtifactInfo } from "@/lib/api"
import { formatConversationTitle } from "@/lib/conversation-title"
import type { DbConversationSummary } from "@/lib/types"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"

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
  searchInput: string
  setSearchInput: Dispatch<SetStateAction<string>>
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
  const [searchInput, setSearchInput] = useState("")
  const [view, setView] = useState<"grid" | "list">("grid")
  const [selected, setSelected] = useState<TaskArtifactInfo | null>(null)
  const conversations = useAppWorkspaceStore((state) => state.conversations)
  useEffect(() => {
    const timer = setTimeout(() => {
      setFilters((value) =>
        value.search === searchInput
          ? value
          : { ...value, search: searchInput, page: 1 }
      )
    }, 250)
    return () => clearTimeout(timer)
  }, [searchInput])
  const conversationId = parseConversationId(filters.session)
  const query = useTaskArtifacts({
    search: filters.search,
    conversationId,
    folderId: null,
    scope: "all",
    page: filters.page,
    pageSize: RESOURCE_PAGE_SIZE,
  })
  const sessionOptions = useSessionOptions(conversations)
  const selection = selected
    ? (query.items.find((item) => item.id === selected.id) ?? selected)
    : null
  return {
    filters,
    setFilters,
    searchInput,
    setSearchInput,
    view,
    setView,
    selected: selection,
    setSelected,
    query,
    displayQuery: query,
    sessionOptions,
    filteredItems: query.items,
  }
}

function ResourcePageContent({ model }: { model: ResourcePageModel }) {
  const {
    filters,
    setFilters,
    searchInput,
    setSearchInput,
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
        searchInput={searchInput}
        setSearchInput={setSearchInput}
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
  searchInput,
  setSearchInput,
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
  searchInput: string
  setSearchInput: Dispatch<SetStateAction<string>>
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
        total={query.total}
        sessionCount={
          filters.session === "all"
            ? sessionOptions.length
            : query.total > 0
              ? 1
              : 0
        }
        todayCount={filteredItems.filter(isToday).length}
        loading={query.loading}
        filtered={filters.session !== "all" || filters.search.trim() !== ""}
      />
      <ResourceToolbar
        search={searchInput}
        onSearchChange={setSearchInput}
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
        filtered={filters.session !== "all"}
        view={view}
        onSelect={onSelect}
        onClear={() => {
          setSearchInput("")
          setFilters({ search: "", session: "all", page: 1 })
        }}
        onPageChange={(page) => setFilters((value) => ({ ...value, page }))}
      />
    </div>
  )
}

function useSessionOptions(
  items: DbConversationSummary[]
): ResourceSessionOption[] {
  return useMemo(
    () =>
      Array.from(
        new Map(
          items.map((item) => [
            item.id,
            {
              id: item.id,
              title: formatConversationTitle(item.title).trim(),
            },
          ])
        ).values()
      ),
    [items]
  )
}

function parseConversationId(session: string): number | null {
  if (session === "all") return null
  const id = Number(session)
  return Number.isInteger(id) && id > 0 ? id : null
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
