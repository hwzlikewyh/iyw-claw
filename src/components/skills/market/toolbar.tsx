"use client"

import { RefreshCw, Search, SlidersHorizontal, Upload, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type { SkillMarketQueryState } from "@/hooks/use-skill-market"
import type {
  SkillMarketCategory,
  SkillMarketTranslator,
  SkillMarketViewV2,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const VIEW_ORDER: SkillMarketViewV2[] = [
  "market",
  "organization",
  "mine",
  "installed",
  "needs_update",
]

export interface SkillMarketToolbarProps {
  query: SkillMarketQueryState
  categories: SkillMarketCategory[]
  revision: string
  offline: boolean
  loading: boolean
  onQueryChange: (patch: Partial<SkillMarketQueryState>) => void
  onRefresh: () => void
  onUpload: () => void
}

function ViewTabs({
  view,
  onChange,
}: {
  view: SkillMarketViewV2
  onChange: (view: SkillMarketViewV2) => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <Tabs
      value={view}
      onValueChange={(value) => onChange(value as SkillMarketViewV2)}
      className="min-w-0"
    >
      <TabsList
        variant="line"
        className="h-8 max-w-full justify-start overflow-x-auto"
      >
        {VIEW_ORDER.map((item) => (
          <TabsTrigger key={item} value={item} className="h-8 flex-none">
            {t(`views.${item}`)}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  )
}

function FilterPopover({
  query,
  categories,
  onQueryChange,
}: {
  query: SkillMarketQueryState
  categories: SkillMarketCategory[]
  onQueryChange: (patch: Partial<SkillMarketQueryState>) => void
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const activeCount =
    (query.publisher !== "all" ? 1 : 0) +
    (query.distribution !== "all" ? 1 : 0) +
    (query.compatibility !== "all" ? 1 : 0) +
    (query.category ? 1 : 0)
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          size="sm"
          variant="outline"
          className={cn(
            "h-8",
            activeCount > 0 && "border-primary/40 text-primary"
          )}
          aria-label={t("filters.label")}
        >
          <SlidersHorizontal className="size-3.5" aria-hidden="true" />
          {t("filters.label")}
          {activeCount > 0 ? (
            <Badge variant="secondary" className="ml-0.5 h-4 min-w-4 px-1">
              {activeCount}
            </Badge>
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 gap-3">
        <div className="grid gap-2">
          <label className="text-xs font-medium text-muted-foreground">
            {t("filters.publisher")}
          </label>
          <Select
            value={query.publisher}
            onValueChange={(value) =>
              onQueryChange({
                publisher: value as SkillMarketQueryState["publisher"],
              })
            }
          >
            <SelectTrigger className="w-full rounded-md">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t("filters.all")}</SelectItem>
              <SelectItem value="official">{t("filters.official")}</SelectItem>
              <SelectItem value="user">{t("filters.user")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-2">
          <label className="text-xs font-medium text-muted-foreground">
            {t("filters.distribution")}
          </label>
          <Select
            value={query.distribution}
            onValueChange={(value) =>
              onQueryChange({
                distribution: value as SkillMarketQueryState["distribution"],
              })
            }
          >
            <SelectTrigger className="w-full rounded-md">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t("filters.all")}</SelectItem>
              <SelectItem value="mandatory">
                {t("filters.mandatory")}
              </SelectItem>
              <SelectItem value="optional">{t("filters.optional")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-2">
          <label className="text-xs font-medium text-muted-foreground">
            {t("filters.compatibility")}
          </label>
          <Select
            value={query.compatibility}
            onValueChange={(value) =>
              onQueryChange({
                compatibility: value as SkillMarketQueryState["compatibility"],
              })
            }
          >
            <SelectTrigger className="w-full rounded-md">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t("filters.all")}</SelectItem>
              <SelectItem value="compatible">
                {t("filters.compatible")}
              </SelectItem>
              <SelectItem value="incompatible">
                {t("filters.incompatible")}
              </SelectItem>
              <SelectItem value="unknown">{t("filters.unknown")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="grid gap-2">
          <label className="text-xs font-medium text-muted-foreground">
            {t("filters.category")}
          </label>
          <Select
            value={query.category ?? "all"}
            onValueChange={(value) =>
              onQueryChange({ category: value === "all" ? null : value })
            }
          >
            <SelectTrigger className="w-full rounded-md">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t("filters.categoryAll")}</SelectItem>
              {categories.map((category) => (
                <SelectItem key={category.key} value={category.key}>
                  {t(`categories.${category.key}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <Button
          size="sm"
          variant="outline"
          className="w-full"
          disabled={activeCount === 0}
          onClick={() =>
            onQueryChange({
              publisher: "all",
              distribution: "all",
              compatibility: "all",
              category: null,
            })
          }
        >
          {t("filters.reset")}
        </Button>
      </PopoverContent>
    </Popover>
  )
}

export function SkillMarketToolbar(props: SkillMarketToolbarProps) {
  const t = useTranslations("SkillMarketV2")
  const { query, onQueryChange } = props
  return (
    <header className="shrink-0 border-b bg-background px-4 py-2.5">
      <div className="flex min-w-0 items-center gap-3">
        <h1 className="shrink-0 text-sm font-semibold">{t("title")}</h1>
        <span className="h-4 w-px shrink-0 bg-border" aria-hidden="true" />
        <ViewTabs
          view={query.view}
          onChange={(view) => onQueryChange({ view })}
        />
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <Button
            size="sm"
            variant="outline"
            className="h-8"
            onClick={props.onUpload}
          >
            <Upload className="size-3.5" aria-hidden="true" />
            {t("uploadShort")}
          </Button>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="icon-sm"
                variant="ghost"
                className="h-8 w-8"
                aria-label={t("refresh.label")}
                title={t("refresh.label")}
                onClick={props.onRefresh}
              >
                <RefreshCw
                  className={cn("size-3.5", props.loading && "animate-spin")}
                  aria-hidden="true"
                />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("refresh.label")}</TooltipContent>
          </Tooltip>
        </div>
      </div>
      <div className="mt-2 flex flex-col gap-2 sm:flex-row sm:items-center">
        <div className="relative min-w-0 flex-1">
          <Search
            className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
            aria-hidden="true"
          />
          <Input
            value={query.q}
            onChange={(event) => onQueryChange({ q: event.target.value })}
            placeholder={t("search.placeholder")}
            aria-label={t("search.placeholder")}
            className="h-8 pl-8 pr-8"
          />
          {query.q ? (
            <Button
              size="icon-xs"
              variant="ghost"
              className="absolute right-1 top-1/2 h-6 w-6 -translate-y-1/2"
              aria-label={t("search.clear")}
              title={t("search.clear")}
              onClick={() => onQueryChange({ q: "" })}
            >
              <X className="size-3.5" aria-hidden="true" />
            </Button>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <FilterPopover
            query={query}
            categories={props.categories}
            onQueryChange={onQueryChange}
          />
          <Select
            value={query.sort}
            onValueChange={(value) =>
              onQueryChange({ sort: value as SkillMarketQueryState["sort"] })
            }
          >
            <SelectTrigger size="sm" className="h-8 w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="recommended">
                {t("sort.recommended")}
              </SelectItem>
              <SelectItem value="updated">{t("sort.updated")}</SelectItem>
              <SelectItem value="name">{t("sort.name")}</SelectItem>
              <SelectItem value="installed">{t("sort.installed")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
      <div className="mt-1.5 flex h-4 items-center gap-2 text-[10px] text-muted-foreground">
        {props.offline ? (
          <Badge variant="outline" className="h-4 px-1 text-[10px]">
            {t("refresh.offline")}
          </Badge>
        ) : null}
        {props.revision ? (
          <span
            className="truncate"
            title={t("refresh.revision", { revision: props.revision })}
          >
            {t("refresh.revision", { revision: props.revision })}
          </span>
        ) : null}
      </div>
    </header>
  )
}
