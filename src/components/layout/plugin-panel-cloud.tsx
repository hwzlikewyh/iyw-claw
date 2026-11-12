"use client"

import { useEffect, useState } from "react"
import { AppWindow } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { getSkillMarketSource } from "@/lib/skill-market-source"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { usePluginMarketNavigation } from "./use-plugin-market-navigation"

const PAGE_SIZE = 20
const readPage = (cursor: string | null) =>
  getSkillMarketSource().list({
    view: "market",
    publisher: "all",
    distribution: "all",
    compatibility: "all",
    category: null,
    q: "",
    sort: "recommended",
    cursor,
    limit: PAGE_SIZE,
  })

function useCloudPlugins() {
  const [page, setPage] = useState<string | null>(null)
  const [revision, setRevision] = useState(0)
  const [state, setState] = useState({
    items: [] as SkillMarketV2Item[],
    cursor: null as string | null,
    loading: true,
    error: false,
  })
  useEffect(() => {
    let alive = true
    const timer = setTimeout(() => {
      setState((value) => ({ ...value, loading: true, error: false }))
      void readPage(page)
        .then((result) => {
          if (alive)
            setState((value) => ({
              items: page
                ? [
                    ...value.items,
                    ...result.items.filter(
                      (item) => !value.items.some((old) => old.id === item.id)
                    ),
                  ]
                : result.items,
              cursor: result.nextCursor,
              loading: false,
              error: false,
            }))
        })
        .catch(() => {
          if (alive)
            setState((value) => ({ ...value, loading: false, error: true }))
        })
    }, 0)
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [page, revision])
  return {
    ...state,
    more: () => setPage(state.cursor),
    refresh: () => setRevision((value) => value + 1),
  }
}

export function CloudPlugins({ installed }: { installed: string[] }) {
  const t = useTranslations("PluginPanels")
  const catalog = useCloudPlugins()
  const plugins = catalog.items.filter(
    (item) =>
      item.packageType === "plugin" &&
      item.installState === "not_installed" &&
      !installed.includes(item.slug)
  )
  return (
    <div className="mt-4 border-t pt-3">
      <p className="mb-2 text-xs text-muted-foreground">{t("cloudHint")}</p>
      <ul className="grid grid-cols-1 gap-x-4 gap-y-1 @min-[480px]:grid-cols-2">
        {plugins.map((item) => (
          <CloudPlugin key={item.id} item={item} />
        ))}
      </ul>
      {catalog.loading && (
        <p role="status" className="py-2 text-sm text-muted-foreground">
          {t("loadingCloud")}
        </p>
      )}
      {catalog.error && (
        <div className="py-2 text-sm">
          <p role="alert" className="text-muted-foreground">
            {t("cloudError")}
          </p>
          <Button size="xs" variant="link" onClick={catalog.refresh}>
            {t("refresh")}
          </Button>
        </div>
      )}
      {catalog.cursor && (
        <Button
          size="sm"
          variant="outline"
          className="mt-2"
          disabled={catalog.loading}
          onClick={catalog.more}
        >
          {t("more")}
        </Button>
      )}
    </div>
  )
}

function CloudPlugin({ item }: { item: SkillMarketV2Item }) {
  const t = useTranslations("PluginPanels")
  const route = usePluginMarketNavigation()
  return (
    <li className="flex min-h-16 min-w-0 items-center gap-3">
      <AppWindow className="size-5 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium" title={item.displayName}>
          {item.displayName}
        </p>
        <p
          className="truncate text-xs text-muted-foreground"
          title={item.summary}
        >
          {item.summary}
        </p>
      </div>
      <Button
        size="xs"
        variant="outline"
        onClick={() => route.openSkillMarket(item.slug)}
      >
        {t("install")}
      </Button>
    </li>
  )
}
