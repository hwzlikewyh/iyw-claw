"use client"

import { RefreshCw, Search, Upload, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
import { FilterPopover } from "@/components/skills/market/filter-popover"
import type { SkillMarketQueryState } from "@/hooks/use-skill-market"
import type { SkillMarketCategory, SkillMarketViewV2 } from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const VIEW_ORDER: SkillMarketViewV2[] = [
  "market",
  "organization",
  "mine",
  "installed",
  "enabled",
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
      className="min-w-0 flex-1 overflow-x-auto overflow-y-hidden [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
    >
      <TabsList
        variant="line"
        className="h-8 w-max justify-start overflow-visible bg-transparent p-0"
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

export function SkillMarketToolbar(props: SkillMarketToolbarProps) {
  const t = useTranslations("SkillMarketV2")
  const { query, onQueryChange } = props
  return (
    <header className="shrink-0 border-b bg-background px-4 py-2.5 sm:px-5">
      <div className="flex min-w-0 items-center gap-3">
        <h2 className="hidden shrink-0 text-sm font-semibold lg:block">
          {t("catalogTitle")}
        </h2>
        <span
          className="hidden h-5 w-px shrink-0 bg-border lg:block"
          aria-hidden="true"
        />
        <ViewTabs
          view={query.view}
          onChange={(view) => onQueryChange({ view })}
        />
        <div className="ml-auto flex shrink-0 items-center justify-end gap-1">
          {props.offline ? (
            <Badge
              variant="outline"
              className="hidden h-7 px-2 text-[10px] sm:inline-flex"
            >
              {t("refresh.offline")}
            </Badge>
          ) : null}
          <Button
            size="icon-sm"
            variant="ghost"
            className="h-8 w-8"
            aria-label={t("uploadShort")}
            title={t("uploadShort")}
            onClick={props.onUpload}
          >
            <Upload className="size-3.5" aria-hidden="true" />
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
            className="h-9 border-border/70 bg-muted/20 pl-8 pr-8 shadow-none focus-visible:bg-background"
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
        <div className="flex shrink-0 items-center justify-end gap-2">
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
            <SelectTrigger size="sm" className="h-9 w-32">
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
    </header>
  )
}
