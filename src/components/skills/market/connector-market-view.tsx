"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, RefreshCw, Search } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MarketItemIcon } from "./market-item-icon"
import { ConnectorInstallDialog } from "./connector-install-dialog"
import { ConnectorInstalledList } from "./connector-installed-list"
import {
  mcpListMarketplaces,
  mcpScanLocal,
  mcpSearchMarketplace,
} from "@/lib/api"
import type { LocalMcpServer, McpMarketplaceItem } from "@/lib/types"

export function ConnectorMarketView() {
  const t = useTranslations("ConnectorMarket")
  const [items, setItems] = useState<McpMarketplaceItem[]>([])
  const [installed, setInstalled] = useState<LocalMcpServer[]>([])
  const [query, setQuery] = useState("")
  const [error, setError] = useState("")
  const [loading, setLoading] = useState(true)
  const [active, setActive] = useState<McpMarketplaceItem | null>(null)
  const [section, setSection] = useState("market")
  const reload = useCallback(async () => {
    setLoading(true)
    setError("")
    try {
      const [providers, local] = await Promise.all([
        mcpListMarketplaces(),
        mcpScanLocal(),
      ])
      setInstalled(
        local.filter(
          (item) =>
            item.spec.iyw_builtin == null &&
            item.id !== "iyw-capability-gateway"
        )
      )
      const provider = providers[0]
      if (!provider) throw new Error(t("unavailable"))
      setItems(
        await mcpSearchMarketplace({ providerId: provider.id, limit: 1000 })
      )
    } catch (cause) {
      setError(String(cause))
    } finally {
      setLoading(false)
    }
  }, [t])
  useEffect(() => {
    void reload()
  }, [reload])
  const filtered = useMemo(
    () =>
      items.filter((item) =>
        `${item.name} ${item.description} ${item.server_id}`
          .toLowerCase()
          .includes(query.toLowerCase())
      ),
    [items, query]
  )
  const installedIds = new Set(installed.map((item) => item.id))
  return (
    <div className="h-full overflow-auto p-4 sm:p-6">
      <div className="mx-auto max-w-6xl space-y-5">
        <header className="flex items-start justify-between gap-3">
          <div>
            <h2 className="text-xl font-semibold">{t("title")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("description")}
            </p>
          </div>
          <Button
            variant="outline"
            size="icon"
            disabled={loading}
            aria-label={t("refresh")}
            onClick={() => void reload()}
          >
            <RefreshCw className="size-4" />
          </Button>
        </header>
        <div className="relative max-w-sm">
          <Search className="absolute left-3 top-2.5 size-4 text-muted-foreground" />
          <Input
            className="pl-9"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("search")}
          />
        </div>
        {error && (
          <p
            role="alert"
            className="rounded-md border border-destructive/30 p-3 text-sm text-destructive"
          >
            {error}
          </p>
        )}
        <Tabs value={section} onValueChange={setSection}>
          <TabsList>
            <TabsTrigger value="market">{t("discover")}</TabsTrigger>
            <TabsTrigger value="installed">
              {t("installed")} ({installed.length})
            </TabsTrigger>
          </TabsList>
          <TabsContent value="market">
            <div className="grid gap-3 pt-3 md:grid-cols-2 xl:grid-cols-3">
              {filtered.map((item) => (
                <article
                  key={item.server_id}
                  className="flex flex-col gap-3 rounded-lg border bg-card p-4"
                >
                  <div className="flex items-center gap-3">
                    <MarketItemIcon name={item.name} src={item.icon_url} />
                    <div className="min-w-0">
                      <h3 className="text-sm font-semibold">{item.name}</h3>
                      <p className="text-xs text-muted-foreground">
                        {item.owner} · {item.latest_version}
                      </p>
                    </div>
                  </div>
                  <p className="line-clamp-3 min-h-12 text-xs text-muted-foreground">
                    {item.description}
                  </p>
                  <div className="flex flex-wrap gap-1">
                    {item.protocols.map((protocol) => (
                      <Badge key={protocol} variant="outline">
                        {protocol === "sse"
                          ? t("legacySse")
                          : protocol === "http"
                            ? "Streamable HTTP"
                            : "STDIO"}
                      </Badge>
                    ))}
                  </div>
                  <Button
                    size="sm"
                    className="mt-auto"
                    variant={
                      installedIds.has(item.server_id) ? "outline" : "default"
                    }
                    onClick={() => setActive(item)}
                  >
                    {installedIds.has(item.server_id)
                      ? t("configure")
                      : t("add")}
                  </Button>
                </article>
              ))}
            </div>
          </TabsContent>
          <TabsContent value="installed">
            <ConnectorInstalledList
              items={installed.filter((item) =>
                `${item.id} ${item.display_name}`
                  .toLowerCase()
                  .includes(query.toLowerCase())
              )}
              onChanged={reload}
            />
          </TabsContent>
        </Tabs>
        {loading && (
          <Loader2 className="size-5 animate-spin" aria-label={t("loading")} />
        )}
        {!loading && section === "market" && !filtered.length && !error && (
          <p className="py-10 text-center text-sm text-muted-foreground">
            {t("empty")}
          </p>
        )}
        {active && (
          <ConnectorInstallDialog
            item={active}
            onClose={() => setActive(null)}
            onInstalled={() => {
              setActive(null)
              toast.success(t("installedHint"))
              void reload()
            }}
          />
        )}
      </div>
    </div>
  )
}
