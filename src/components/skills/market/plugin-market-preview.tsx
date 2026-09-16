"use client"

import { CircleAlert, SearchX } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"
import { PluginMarketCard } from "./plugin-market-card"
import { PluginMarketToolbar } from "./plugin-market-toolbar"
import { usePluginCatalog } from "./use-plugin-catalog"

const CATALOG_GRID =
  "grid grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-3 p-4 pt-2 sm:p-5 sm:pt-2"

export function PluginMarketPreview({
  onOpenPlugin,
}: {
  onOpenPlugin: (slug: string) => void
}) {
  const t = useTranslations("CapabilityMarket.plugins")
  const [view, setView] = useState<"market" | "organization">("market")
  const [query, setQuery] = useState("")
  const catalog = usePluginCatalog(view)
  const items = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase()
    return catalog.items.filter((item) =>
      [item.displayName, item.slug, item.summary, ...item.tags]
        .join(" ")
        .toLocaleLowerCase()
        .includes(needle)
    )
  }, [catalog.items, query])
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <PluginMarketToolbar
        view={view}
        query={query}
        loading={catalog.loading}
        onViewChange={setView}
        onQueryChange={setQuery}
        onRefresh={catalog.refresh}
      />
      <div
        className="flex h-8 shrink-0 items-end px-4 pb-1 text-[10px] text-muted-foreground sm:px-5"
        aria-live="polite"
      >
        {catalog.loading
          ? t("loading")
          : t("catalogCount", { count: items.length })}
      </div>
      <PluginCatalogBody
        catalog={catalog}
        items={items}
        onOpenPlugin={onOpenPlugin}
      />
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
  onOpenPlugin: (slug: string) => void
}) {
  if (!catalog.loading && (catalog.error || !items.length)) {
    return (
      <PluginCatalogState error={catalog.error} onRetry={catalog.refresh} />
    )
  }
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className={CATALOG_GRID} aria-busy={catalog.loading}>
        {catalog.loading
          ? Array.from({ length: 6 }, (_, index) => (
              <Skeleton key={index} className="h-52 rounded-lg" />
            ))
          : items.map((item) => (
              <PluginMarketCard
                key={item.id}
                item={item}
                onOpen={() => onOpenPlugin(item.slug)}
              />
            ))}
      </div>
    </ScrollArea>
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
