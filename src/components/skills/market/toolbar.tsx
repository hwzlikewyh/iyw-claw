"use client"
import { RefreshCw, Search, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { FilterPopover } from "./filter-popover"
import { MarketQuickPages } from "./market-quick-pages"
import type { MarketPaginationProps } from "./market-pagination"
import type { SkillMarketQueryState } from "@/hooks/use-skill-market"
import type { SkillMarketCategory } from "@/lib/skill-market"

export interface SkillMarketToolbarProps {
  query: SkillMarketQueryState
  categories: SkillMarketCategory[]
  revision: string
  offline: boolean
  loading: boolean
  onQueryChange: (patch: Partial<SkillMarketQueryState>) => void
  onRefresh: () => void
  onUpload: () => void
  pagination?: MarketPaginationProps
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
    <>
      <select
        className="h-8 min-w-0 flex-1 rounded-md border bg-background px-2 text-sm sm:hidden"
        aria-label={t("viewLabel")}
        value={view}
        onChange={(event) => onChange(event.target.value as SkillMarketViewV2)}
      >
        {VIEW_ORDER.map((item) => (
          <option key={item} value={item}>
            {t(item === "market" ? "audience.globalMarket" : `views.${item}`)}
          </option>
        ))}
      </select>
      <Tabs
        value={view}
        onValueChange={(value) => onChange(value as SkillMarketViewV2)}
        className="hidden min-w-0 flex-1 overflow-x-auto overflow-y-hidden [scrollbar-width:none] sm:flex [&::-webkit-scrollbar]:hidden"
      >
        <TabsList
          variant="line"
          className="h-8 w-max justify-start overflow-visible bg-transparent p-0"
        >
          {VIEW_ORDER.map((item) => (
            <TabsTrigger key={item} value={item} className="h-8 flex-none">
              {t(item === "market" ? "audience.globalMarket" : `views.${item}`)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </>
  )
}

export function SkillMarketToolbar(props: SkillMarketToolbarProps) {
  const t = useTranslations("SkillMarketV2")
  const c = useTranslations("CapabilityMarket.catalog")
  const { query, onQueryChange } = props
  const special = ["mine", "installed", "enabled", "needs_update"].includes(
    query.view
  )
  const source =
    query.view === "organization" ? "organization" : query.publisher
  return (
    <div className="my-[18px] flex min-w-0 flex-wrap items-center gap-3">
      {special && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onQueryChange({ view: "all", publisher: "all" })}
        >
          {c("browse")}
        </Button>
      )}
      <div className="relative min-w-[170px] max-w-[330px] flex-1 max-sm:min-w-full max-sm:max-w-none">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={query.q}
          onChange={(e) => onQueryChange({ q: e.target.value })}
          placeholder={t("search.placeholder")}
          aria-label={t("search.placeholder")}
          className="h-9 rounded-md border-input bg-background pl-9 pr-8 shadow-none max-sm:min-h-11"
        />
        <ViewTabs
          view={query.view}
          onChange={(view) => onQueryChange({ view })}
        />
        <div className="ml-auto flex shrink-0 items-center justify-end gap-1">
          {props.offline ? (
            <Badge
              variant="outline"
              className="hidden h-7 px-2 text-[11px] sm:inline-flex"
            >
              {t("refresh.offline")}
            </Badge>
          ) : null}
          <Button
            variant="ghost"
            size="icon-xs"
            className="absolute right-1 top-1/2 -translate-y-1/2"
            aria-label={t("search.clear")}
            onClick={() => onQueryChange({ q: "" })}
          >
            <X className="size-3.5" />
          </Button>
        )}
      </div>
      <select
        aria-label={c("source")}
        value={special ? query.view : source}
        className="h-9 max-w-full rounded-md border bg-background px-3 text-sm max-sm:min-h-11"
        onChange={(e) => {
          const value = e.target.value
          if (["mine", "installed", "enabled", "needs_update"].includes(value))
            onQueryChange({ view: value as SkillMarketQueryState["view"] })
          else
            onQueryChange({
              view: value === "organization" ? "organization" : "all",
              publisher:
                value === "organization"
                  ? "all"
                  : (value as SkillMarketQueryState["publisher"]),
            })
        }}
      >
        <option value="all">{c("allSources")}</option>
        <option value="official">{c("official")}</option>
        <option value="user">{c("community")}</option>
        <option value="organization">{c("organization")}</option>
        {(["mine", "installed", "enabled", "needs_update"] as const).map(
          (view) => (
            <option key={view} value={view}>
              {t(`views.${view}`)}
            </option>
          )
        )}
      </select>
      <FilterPopover
        query={query}
        categories={props.categories}
        onQueryChange={onQueryChange}
      />
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={props.loading}
        aria-label={t("refresh.label")}
        onClick={props.onRefresh}
      >
        <RefreshCw className="size-4" />
      </Button>
      {props.offline && (
        <span role="status" className="text-xs text-muted-foreground">
          {t("refresh.offline")}
        </span>
      )}
      {props.pagination && <MarketQuickPages {...props.pagination} />}
    </div>
  )
}
