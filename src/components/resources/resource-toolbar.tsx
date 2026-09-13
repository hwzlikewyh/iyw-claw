"use client"

import { LayoutGrid, List, RefreshCw, Search, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"

export interface ResourceSessionOption {
  id: number
  title: string
}

interface ResourceToolbarProps {
  search: string
  onSearchChange: (value: string) => void
  session: string
  sessionOptions: ResourceSessionOption[]
  onSessionChange: (value: string) => void
  view: "grid" | "list"
  onViewChange: (value: "grid" | "list") => void
  busy: boolean
  onRefresh: () => void
}

export function ResourceToolbar(props: ResourceToolbarProps) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-3 border-b pb-4">
      <ResourceSearch {...props} />
      <ResourceSessionFilter {...props} />
      <div className="ms-auto flex shrink-0 items-center gap-2">
        <ResourceViewSwitch {...props} />
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={props.busy}
          onClick={props.onRefresh}
          aria-label={t("refresh")}
          title={t("refresh")}
        >
          <RefreshCw className={cn("size-4", props.busy && "animate-spin")} />
        </Button>
      </div>
    </div>
  )
}

function ResourceSessionFilter({
  session,
  sessionOptions,
  onSessionChange,
}: ResourceToolbarProps) {
  const t = useTranslations("Resources")
  const artifactT = useTranslations("Folder.taskArtifacts")
  return (
    <select
      value={session}
      onChange={(event) => onSessionChange(event.target.value)}
      aria-label={t("sessionFilterLabel")}
      className="h-9 min-w-40 max-w-full rounded-md border bg-muted/25 px-3 text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <option value="all">{t("allConversations")}</option>
      {sessionOptions.map((option) => (
        <option key={option.id} value={String(option.id)}>
          {option.title || artifactT("untitled")}
        </option>
      ))}
    </select>
  )
}

function ResourceSearch({ search, onSearchChange }: ResourceToolbarProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const r = useTranslations("Resources")
  return (
    <div className="relative min-w-0 basis-full sm:basis-80">
      <Search className="pointer-events-none absolute start-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        value={search}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={t("searchPlaceholder")}
        aria-label={t("searchLabel")}
        className="h-9 rounded-md bg-muted/25 ps-9 pe-9 text-sm"
      />
      {search && (
        <Button
          variant="ghost"
          size="icon-xs"
          className="absolute end-1.5 top-1/2 -translate-y-1/2"
          onClick={() => onSearchChange("")}
          aria-label={r("clearSearch")}
          title={r("clearSearch")}
        >
          <X className="size-3.5" />
        </Button>
      )}
    </div>
  )
}

function ResourceViewSwitch({ view, onViewChange }: ResourceToolbarProps) {
  const t = useTranslations("Resources")
  return (
    <div
      className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
      role="group"
      aria-label={t("viewLabel")}
    >
      {(
        [
          ["grid", LayoutGrid],
          ["list", List],
        ] as const
      ).map(([value, Icon]) => (
        <Button
          key={value}
          variant="ghost"
          size="icon-sm"
          className={cn(
            "rounded-sm text-muted-foreground",
            view === value && "bg-background text-foreground shadow-xs"
          )}
          aria-pressed={view === value}
          aria-label={t(value)}
          title={t(value)}
          onClick={() => onViewChange(value)}
        >
          <Icon className="size-4" />
        </Button>
      ))}
    </div>
  )
}
