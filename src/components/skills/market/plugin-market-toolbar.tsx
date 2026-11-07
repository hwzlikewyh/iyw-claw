"use client"
import { RefreshCw, Search } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { MarketQuickPages } from "./market-quick-pages"
import type { MarketPaginationProps } from "./market-pagination"
export type PluginCatalogView = "all" | "market" | "organization"
interface Props {
  view: PluginCatalogView
  query: string
  publisher: "all" | "official" | "user"
  loading: boolean
  onViewChange: (view: PluginCatalogView) => void
  onPublisherChange: (publisher: Props["publisher"]) => void
  onQueryChange: (query: string) => void
  onRefresh: () => void
  pagination: MarketPaginationProps
}
export function PluginMarketToolbar(props: Props) {
  const t = useTranslations("SkillMarketV2"),
    c = useTranslations("CapabilityMarket.catalog")
  return (
    <div className="my-[18px] flex flex-wrap items-center gap-3">
      <div className="relative min-w-[170px] max-w-[330px] flex-1 max-sm:min-w-full max-sm:max-w-none">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={props.query}
          onChange={(e) => props.onQueryChange(e.target.value)}
          placeholder={t("search.placeholder")}
          aria-label={t("search.placeholder")}
          className="h-9 rounded-md bg-background pl-9 shadow-none max-sm:min-h-11"
        />
      </div>
      <select
        className="h-9 rounded-md border bg-background px-3 text-sm max-sm:min-h-11"
        aria-label={c("source")}
        value={props.view === "organization" ? "organization" : props.publisher}
        onChange={(e) => {
          const value = e.target.value
          props.onViewChange(value === "organization" ? "organization" : "all")
          props.onPublisherChange(
            value === "organization" ? "all" : (value as Props["publisher"])
          )
        }}
      >
        <option value="all">{c("allSources")}</option>
        <option value="official">{c("official")}</option>
        <option value="user">{c("community")}</option>
        <option value="organization">{c("organization")}</option>
      </select>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={props.loading}
        aria-label={t("refresh.label")}
        onClick={props.onRefresh}
      >
        <RefreshCw className="size-4" />
      </Button>
      <MarketQuickPages {...props.pagination} />
    </div>
  )
}
