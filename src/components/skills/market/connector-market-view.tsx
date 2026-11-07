"use client"

import { useEffect, useRef, useState } from "react"
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
import type { McpMarketplaceItem } from "@/lib/types"
import { useConnectorMarket } from "./use-connector-market"
import { useConnectorInstallTasks } from "./use-connector-install-tasks"
import { ConnectorInstallTasks } from "./connector-install-tasks"
import { connectorTaskActive } from "@/lib/connector-install-tasks"
import { useIywAccount } from "@/contexts/iyw-account-context"

export function ConnectorMarketView() {
  const { profile, status } = useIywAccount()
  const scope = JSON.stringify([
    status,
    profile?.user_id,
    profile?.org_code,
    profile?.workspace_pending,
  ])
  return <ConnectorMarketContent key={scope} />
}

function ConnectorMarketContent() {
  const t = useTranslations("ConnectorMarket")
  const {
    items,
    installed,
    query,
    setQuery,
    error,
    loading,
    page,
    setPage,
    hasMore,
    reload,
  } = useConnectorMarket(t("unavailable"))
  const [active, setActive] = useState<McpMarketplaceItem | null>(null)
  const [section, setSection] = useState("market")
  const tasks = useConnectorInstallTasks()
  const settled = useRef<Set<string> | null>(null)
  useEffect(() => {
    if (!tasks.loaded) {
      settled.current = null
      return
    }
    const done = tasks.tasks.filter((task) => !connectorTaskActive(task))
    if (!settled.current) {
      settled.current = new Set(done.map((task) => task.id))
      return
    }
    const fresh = done.filter((task) => !settled.current?.has(task.id))
    fresh.forEach((task) => settled.current?.add(task.id))
    if (fresh.length) {
      void reload()
      if (fresh.some((task) => task.phase === "completed"))
        toast.success(t("installedHint"))
    }
  }, [tasks.tasks, tasks.loaded, reload, t])
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
        {tasks.error ? (
          <div
            role="alert"
            className="flex flex-wrap items-center gap-3 text-sm text-destructive"
          >
            <span>{tasks.error}</span>
            <Button variant="outline" size="sm" onClick={tasks.refresh}>
              {t("retry")}
            </Button>
          </div>
        ) : null}
        <ConnectorInstallTasks
          tasks={tasks.tasks}
          items={items}
          onOpen={setActive}
        />
        <Tabs value={section} onValueChange={setSection}>
          <TabsList>
            <TabsTrigger value="market">{t("discover")}</TabsTrigger>
            <TabsTrigger value="installed">
              {t("installed")} ({installed.length})
            </TabsTrigger>
          </TabsList>
          <TabsContent value="market">
            <div className="grid gap-3 pt-3 md:grid-cols-2 xl:grid-cols-3">
              {items.map((item) => (
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
            <div className="flex items-center justify-center gap-3 pt-5">
              <Button
                size="sm"
                variant="outline"
                disabled={loading || page === 0}
                onClick={() => setPage(page - 1)}
              >
                {t("previousPage")}
              </Button>
              <span className="text-xs text-muted-foreground">
                {t("pageNumber", { page: page + 1 })}
              </span>
              <Button
                size="sm"
                variant="outline"
                disabled={loading || !hasMore}
                onClick={() => setPage(page + 1)}
              >
                {t("nextPage")}
              </Button>
            </div>
          </TabsContent>
          <TabsContent value="installed">
            <ConnectorInstalledList
              key={tasks.scope}
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
        {!loading && section === "market" && !items.length && !error && (
          <p className="py-10 text-center text-sm text-muted-foreground">
            {t("empty")}
          </p>
        )}
        {active && (
          <ConnectorInstallDialog
            key={`${tasks.scope}:${active.server_id}`}
            item={active}
            installed={installedIds.has(active.server_id)}
            task={tasks.tasks.find(
              (task) => task.serverId === active.server_id
            )}
            onClose={() => setActive(null)}
            onTaskChanged={tasks.refresh}
            onManageInstalled={() => {
              setActive(null)
              setSection("installed")
              void reload()
            }}
          />
        )}
      </div>
    </div>
  )
}
