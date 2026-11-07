"use client"

import { CircleAlert, SearchX } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useEffect, useRef } from "react"
import { Skeleton } from "@/components/ui/skeleton"
import { PluginMarketCard } from "./plugin-market-card"
import { PluginMarketToolbar } from "./plugin-market-toolbar"
import { usePluginCatalog } from "./use-plugin-catalog"
import { usePluginCatalogPage } from "./use-plugin-catalog-page"
import { MarketPagination } from "./market-pagination"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import type { PluginCatalogView } from "./plugin-market-toolbar"

const CATALOG_GRID =
  "grid grid-cols-1 gap-4 @min-[560px]:grid-cols-2 @min-[960px]:grid-cols-3"

export function PluginMarketPreview({
  onOpenPlugin,
  onViewChange: onParentViewChange,
}: {
  onOpenPlugin: (item: SkillMarketV2Item) => void
  onViewChange?: (view: PluginCatalogView) => void
}) {
  const {
    view,
    publisher,
    query,
    catalog,
    items,
    pagination,
    onViewChange,
    onQueryChange,
    onPublisherChange,
  } = usePluginCatalogPage()
  const root = useRef<HTMLDivElement>(null)
  const previousPage = useRef(pagination.page)
  useEffect(() => {
    if (catalog.loading || previousPage.current === pagination.page) return
    previousPage.current = pagination.page
    root.current?.closest("[data-market-scroll]")?.scrollTo({ top: 0 })
    root.current
      ?.querySelector<HTMLButtonElement>("[data-market-card-title]")
      ?.focus({ preventScroll: true })
  }, [catalog.loading, pagination.page])
  return (
    <div ref={root} className="@container min-w-0 bg-background">
      <PluginMarketToolbar
        view={view}
        publisher={publisher}
        onPublisherChange={onPublisherChange}
        query={query}
        loading={catalog.loading}
        onViewChange={(next) => {
          onViewChange(next)
          onParentViewChange?.(next)
        }}
        onQueryChange={onQueryChange}
        onRefresh={catalog.refresh}
        pagination={pagination}
      />
      <PluginCatalogBody
        key={`${view}:${query}:${pagination.page}:${pagination.pageSize}`}
        catalog={catalog}
        items={items}
        onOpenPlugin={onOpenPlugin}
      />
      {!catalog.error && <MarketPagination {...pagination} />}
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
    <div data-market-grid className={CATALOG_GRID} aria-busy={catalog.loading}>
      {catalog.loading
        ? Array.from({ length: 6 }, (_, index) => (
            <Skeleton
              data-market-skeleton
              key={index}
              className="h-52 rounded-lg"
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
