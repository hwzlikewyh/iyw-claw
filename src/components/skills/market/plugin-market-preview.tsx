"use client"

import { CircleAlert, SearchX } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useMarketPageFocus } from "./use-market-page-focus"
import { Skeleton } from "@/components/ui/skeleton"
import { PluginMarketCard } from "./plugin-market-card"
import { PluginMarketToolbar } from "./plugin-market-toolbar"
import { usePluginCatalog } from "./use-plugin-catalog"
import { usePluginCatalogPage } from "./use-plugin-catalog-page"
import { MarketPagination } from "./market-pagination"

const CATALOG_GRID =
  "grid grid-cols-1 gap-3 p-4 @min-[480px]:grid-cols-2 @min-[840px]:grid-cols-3 sm:p-5"

export function PluginMarketPreview({
  onOpenPlugin,
  onViewChange: onParentViewChange,
}: {
  onOpenPlugin: (item: SkillMarketV2Item) => void
  onViewChange?: (view: PluginCatalogView) => void
}) {
  const t = useTranslations("CapabilityMarket.plugins")
  const {
    view,
    query,
    catalog,
    items,
    pagination,
    onViewChange,
    onQueryChange,
  } = usePluginCatalogPage()
  return (
    <div className="@container flex h-full min-h-0 flex-col bg-background">
      <PluginMarketToolbar
        view={view}
        publisher={publisher}
        onPublisherChange={onPublisherChange}
        query={query}
        loading={catalog.loading}
        onViewChange={onViewChange}
        onQueryChange={onQueryChange}
        onRefresh={catalog.refresh}
        pagination={pagination}
      />
      <div
        className="flex h-8 shrink-0 items-end px-4 pb-1 text-[11px] text-muted-foreground sm:px-5"
        aria-live="polite"
      >
        {catalog.loading
          ? t("loading")
          : t("catalogCount", { count: pagination.total })}
      </div>
      <PluginCatalogBody
        key={`${view}:${query}:${pagination.page}:${pagination.pageSize}`}
        catalog={catalog}
        items={items}
        onOpenPlugin={onOpenPlugin}
      />
      <MarketPagination {...pagination} />
    </div>
  )
}

function PluginCatalogBody({
  catalog,
  items,
  onOpenPlugin,
}: {
  catalog: ReturnType<typeof usePluginCatalog>
  items: ReturnType<typeof usePluginCatalog>["items"]
  onOpenPlugin: (item: SkillMarketV2Item) => void
}) {
  if (!catalog.loading && (catalog.error || !items.length)) {
    return (
      <PluginCatalogState error={catalog.error} onRetry={catalog.refresh} />
    )
  }
  return (
    <div
      data-market-grid
      className={CAPABILITY_GRID_CLASS}
      aria-busy={catalog.loading}
    >
      {catalog.loading
        ? Array.from({ length: 6 }, (_, index) => (
            <Skeleton
              data-market-skeleton
              key={index}
              className="h-44 rounded-lg"
            />
          ))
        : items.map((item) => (
            <PluginMarketCard
              key={item.id}
              item={item}
              onOpen={() => onOpenPlugin(item)}
            />
          ))}
    </div>
  )
}

function PluginCatalogState({
  error,
  onRetry,
}: {
  error: string | null
  onRetry: () => void
}) {
  const t = useTranslations("CapabilityMarket.plugins")
  const Icon = error ? CircleAlert : SearchX
  return (
    <div
      data-market-result-state
      className="flex min-h-48 flex-1 flex-col items-center justify-center gap-3 p-6 text-center"
      role={error ? "alert" : "status"}
    >
      <Icon className="size-6 text-muted-foreground" aria-hidden="true" />
      <p className="max-w-lg break-words text-xs text-muted-foreground">
        {error || t("empty")}
      </p>
      {error ? (
        <Button size="sm" variant="outline" onClick={onRetry}>
          {t("retry")}
        </Button>
      ) : null}
    </div>
  )
}
