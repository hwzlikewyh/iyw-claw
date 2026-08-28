"use client"

import {
  ArrowUpRight,
  Blocks,
  Braces,
  CircleAlert,
  Loader2,
  Package,
  PlugZap,
  Sparkles,
} from "lucide-react"
import { useCallback, useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { getSkillMarketSource } from "@/lib/skill-market-source"
import type { SkillMarketV2Item } from "@/lib/skill-market"

export function PluginMarketPreview({
  onOpenPlugin,
}: {
  onOpenPlugin: (slug: string) => void
}) {
  return (
    <ScrollArea className="h-full">
      <div className="px-4 py-5 sm:px-6 lg:py-6">
        <PluginHero />
        <PluginCatalog onOpenPlugin={onOpenPlugin} />
      </div>
    </ScrollArea>
  )
}

function PluginHero() {
  const t = useTranslations("CapabilityMarket.plugins")
  return (
    <section className="-mx-4 -mt-5 grid gap-6 border-b bg-muted/[0.16] px-4 py-6 sm:-mx-6 sm:px-6 lg:-mt-6 lg:grid-cols-[minmax(0,1fr)_26rem] lg:items-center">
      <div className="max-w-2xl">
        <span className="inline-flex items-center gap-1.5 text-[10px] font-semibold uppercase text-primary">
          <Blocks className="size-3" aria-hidden="true" />
          {t("eyebrow")}
        </span>
        <h2 className="mt-3 text-lg font-semibold">{t("title")}</h2>
        <p className="mt-2 max-w-xl text-sm leading-6 text-muted-foreground">
          {t("description")}
        </p>
      </div>
      <div className="grid grid-cols-3 gap-2 sm:gap-3">
        <PackagePart icon={Sparkles} label={t("parts.skills")} />
        <PackagePart icon={PlugZap} label={t("parts.connectors")} />
        <PackagePart icon={Braces} label={t("parts.bindings")} />
      </div>
    </section>
  )
}

function PluginCatalog({
  onOpenPlugin,
}: {
  onOpenPlugin: (slug: string) => void
}) {
  const t = useTranslations("CapabilityMarket.plugins")
  const [items, setItems] = useState<SkillMarketV2Item[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(() => {
    setLoading(true)
    setError(null)
    void getSkillMarketSource()
      .list({
        view: "market",
        publisher: "all",
        distribution: "all",
        compatibility: "all",
        category: null,
        q: "",
        sort: "updated",
        cursor: null,
        limit: 50,
      })
      .then((result) =>
        setItems(result.items.filter((item) => item.packageType === "plugin"))
      )
      .catch((reason: unknown) =>
        setError(reason instanceof Error ? reason.message : String(reason))
      )
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => {
    load()
  }, [load])
  return (
    <section className="pt-7">
      <div className="mb-4 flex items-end justify-between gap-4">
        <div>
          <h3 className="text-sm font-semibold">{t("catalogTitle")}</h3>
          <p className="mt-1 text-[11px] text-muted-foreground">
            {t("catalogSubtitle")}
          </p>
        </div>
        <span className="text-[10px] text-muted-foreground">
          {loading ? t("loading") : t("catalogCount", { count: items.length })}
        </span>
      </div>
      {error ? (
        <div className="flex items-center gap-2 border p-4 text-xs text-muted-foreground">
          <CircleAlert className="size-4" aria-hidden="true" />
          <span className="min-w-0 flex-1">{error}</span>
          <Button size="xs" variant="outline" onClick={load}>
            {t("retry")}
          </Button>
        </div>
      ) : loading ? (
        <div className="flex items-center gap-2 p-4 text-xs text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          {t("loading")}
        </div>
      ) : items.length ? (
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {items.map((item) => (
            <PluginCard
              key={item.id}
              item={item}
              onOpen={() => onOpenPlugin(item.slug)}
            />
          ))}
        </div>
      ) : (
        <div className="border p-6 text-center text-xs text-muted-foreground">
          {t("empty")}
        </div>
      )}
    </section>
  )
}

function PluginCard({
  item,
  onOpen,
}: {
  item: SkillMarketV2Item
  onOpen: () => void
}) {
  const t = useTranslations("CapabilityMarket.plugins")
  const plugin = item.currentVersion.plugin
  const components = plugin?.components ?? []
  const status = item.currentVersion.status
  return (
    <button
      type="button"
      className="group flex min-h-44 flex-col rounded-lg border bg-background p-4 text-left transition-[border-color,box-shadow,transform] hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      aria-label={t("openDetail", { name: item.displayName })}
      onClick={onOpen}
    >
      <div className="flex items-start justify-between gap-3">
        <span className="flex size-9 items-center justify-center rounded-md bg-primary/10">
          <Package className="size-4" aria-hidden="true" />
        </span>
        <span className="text-[10px] font-medium text-muted-foreground">
          {t(`status.${status}`)}
        </span>
      </div>
      <span className="mt-4 flex min-w-0 items-center gap-1 text-sm font-semibold group-hover:underline">
        <span className="truncate">{item.displayName}</span>
        <ArrowUpRight className="size-3.5 shrink-0" aria-hidden="true" />
      </span>
      <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
        {item.summary}
      </p>
      <div className="mt-2 flex gap-3 text-[10px] text-muted-foreground">
        <span>
          {t("publisher")}: {t(`publisherValues.${item.publisher}`)}
        </span>
        <span>
          {t("installState")}: {t(`installValues.${item.installState}`)}
        </span>
      </div>
      <div className="mt-auto flex flex-wrap gap-1.5 pt-4">
        {components.slice(0, 4).map((component) => (
          <span
            key={`${component.type}:${component.key}`}
            className="rounded border bg-muted/25 px-2 py-1 text-[10px] text-muted-foreground"
          >
            {t(`componentType.${component.type}`)}
          </span>
        ))}
        <span className="ml-auto text-[10px] text-muted-foreground">
          v{item.currentVersion.version}
        </span>
      </div>
    </button>
  )
}

function PackagePart({
  icon: Icon,
  label,
}: {
  icon: typeof Sparkles
  label: string
}) {
  return (
    <div className="flex min-w-0 flex-col items-center gap-2 rounded-md border bg-background px-2 py-3 text-center">
      <Icon className="size-4" aria-hidden="true" />
      <span className="truncate text-[11px] font-medium">{label}</span>
    </div>
  )
}
