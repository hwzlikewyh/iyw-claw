"use client"

import {
  AlertCircle,
  ChevronLeft,
  ChevronRight,
  MessageSquare,
  PackageOpen,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { TaskArtifactFileRow } from "@/components/layout/task-artifact-file-row"
import { TaskArtifactState } from "@/components/layout/task-artifacts-browser"
import type { useTaskArtifacts } from "@/components/layout/use-task-artifacts"
import { ResourceCard } from "@/components/resources/resource-card"
import { Button } from "@/components/ui/button"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import type { TaskArtifactInfo } from "@/lib/api"

interface ResourceResultsProps {
  query: ReturnType<typeof useTaskArtifacts>
  search: string
  view: "grid" | "list"
  onSelect: (item: TaskArtifactInfo) => void
  onClear: () => void
  onPageChange: (page: number) => void
}

export function ResourceResults(props: ResourceResultsProps) {
  const { query, search } = props
  const t = useTranslations("Folder.taskArtifacts")
  const r = useTranslations("Resources")
  if (query.loading) return <ResourceLoading />
  if (query.error) {
    return (
      <TaskArtifactState
        icon={<AlertCircle className="size-6 text-destructive" />}
        text={t("loadFailed")}
        action={t("retry")}
        onAction={() => void query.refresh()}
      />
    )
  }
  if (query.items.length === 0) {
    return (
      <TaskArtifactState
        icon={<PackageOpen className="size-8" />}
        text={search.trim() ? t("emptySearch") : r("empty")}
        action={search.trim() ? r("clearSearch") : undefined}
        onAction={props.onClear}
      />
    )
  }
  return (
    <>
      <ResourceItems key={`${search}:${query.page}`} {...props} />
      <ResourcePagination {...props} />
    </>
  )
}

function ResourceItems({ query, view, onSelect }: ResourceResultsProps) {
  const { openConversations } = useWorkbenchRoute()
  const t = useTranslations("Folder.taskArtifacts")
  const r = useTranslations("Resources")
  return (
    <div className="min-h-0 flex-1 overflow-y-auto py-5">
      {view === "grid" ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-4">
          {query.items.map((item) => (
            <ResourceCard key={item.id} item={item} onSelect={onSelect} />
          ))}
        </div>
      ) : (
        <div className="divide-y divide-border/60">
          {query.items.map((item) => (
            <div key={item.id} className="py-1">
              <div className="flex items-center gap-1.5 px-2 pt-1 text-xs text-muted-foreground">
                <MessageSquare className="size-3 shrink-0" aria-hidden="true" />
                <span className="shrink-0 text-[10px]">
                  {r("fromConversation")}
                </span>
                <span className="truncate">
                  {item.conversationTitle || t("untitled")}
                </span>
              </div>
              <TaskArtifactFileRow
                item={item}
                onSelect={onSelect}
                onOpenWorkspace={openConversations}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function ResourcePagination({ query, onPageChange }: ResourceResultsProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const pages = Math.max(1, Math.ceil(query.total / query.pageSize))
  const start = (query.page - 1) * query.pageSize + 1
  const end = Math.min(query.page * query.pageSize, query.total)
  return (
    <nav
      className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t py-3 text-xs text-muted-foreground"
      aria-label={t("paginationLabel")}
    >
      <span>{t("paginationSummary", { start, end, total: query.total })}</span>
      <div className="flex items-center gap-2">
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={query.page <= 1 || query.refreshing}
          onClick={() => onPageChange(query.page - 1)}
          aria-label={t("paginationPrevious")}
          title={t("paginationPrevious")}
        >
          <ChevronLeft className="size-4" />
        </Button>
        <span className="min-w-16 text-center tabular-nums">
          {t("paginationPage", { page: query.page, pages })}
        </span>
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={query.page >= pages || query.refreshing}
          onClick={() => onPageChange(query.page + 1)}
          aria-label={t("paginationNext")}
          title={t("paginationNext")}
        >
          <ChevronRight className="size-4" />
        </Button>
      </div>
    </nav>
  )
}

function ResourceLoading() {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="min-h-0 flex-1 overflow-hidden py-5" role="status">
      <span className="sr-only">{t("loading")}</span>
      <div
        className="grid animate-pulse grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-4 motion-reduce:animate-none"
        aria-hidden="true"
      >
        {Array.from({ length: 8 }, (_, index) => (
          <div key={index} className="overflow-hidden rounded-lg border">
            <div className="aspect-[16/9] bg-muted/60" />
            <div className="space-y-3 p-4">
              <div className="h-4 w-3/4 rounded bg-muted" />
              <div className="h-3 w-1/2 rounded bg-muted/60" />
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
