"use client"

import { RefreshCw, Search, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"

interface PluginMarketToolbarProps {
  view: "market" | "organization"
  query: string
  loading: boolean
  onViewChange: (view: "market" | "organization") => void
  onQueryChange: (query: string) => void
  onRefresh: () => void
}

export function PluginMarketToolbar({
  view,
  query,
  loading,
  onViewChange,
  onQueryChange,
  onRefresh,
}: PluginMarketToolbarProps) {
  const t = useTranslations("SkillMarketV2")
  const plugins = useTranslations("CapabilityMarket.plugins")
  return (
    <header className="shrink-0 border-b px-4 py-2.5 sm:px-5">
      <div className="flex min-w-0 items-center gap-3">
        <h2 className="hidden border-r pr-3 text-sm font-semibold lg:block">
          {plugins("catalogTitle")}
        </h2>
        <Tabs
          value={view}
          onValueChange={(value) => onViewChange(value as typeof view)}
        >
          <TabsList variant="line" className="h-8 p-0">
            <TabsTrigger value="market">
              {t("audience.globalMarket")}
            </TabsTrigger>
            <TabsTrigger value="organization">
              {t("audience.organization")}
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <Button
          size="icon-sm"
          variant="ghost"
          className="ml-auto"
          disabled={loading}
          aria-label={t("refresh.label")}
          title={t("refresh.label")}
          onClick={onRefresh}
        >
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
        </Button>
      </div>
      <PluginSearch query={query} onQueryChange={onQueryChange} />
    </header>
  )
}

function PluginSearch({
  query,
  onQueryChange,
}: {
  query: string
  onQueryChange: (query: string) => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="relative mt-2">
      <Search className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <Input
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder={t("search.placeholder")}
        aria-label={t("search.placeholder")}
        className="h-9 bg-muted/20 pl-8 pr-8 shadow-none"
      />
      {query ? (
        <Button
          size="icon-xs"
          variant="ghost"
          className="absolute right-1 top-1/2 -translate-y-1/2"
          aria-label={t("search.clear")}
          title={t("search.clear")}
          onClick={() => onQueryChange("")}
        >
          <X className="size-3.5" />
        </Button>
      ) : null}
    </div>
  )
}
