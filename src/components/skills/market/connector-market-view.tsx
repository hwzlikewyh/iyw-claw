"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, RefreshCw, Search } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { CapabilityCard, CAPABILITY_GRID_CLASS } from "./capability-card"
import { MarketPagination } from "./market-pagination"
import { MarketQuickPages } from "./market-quick-pages"
import {
  useMarketViewAction,
  type MarketViewAction,
} from "./market-view-action"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { ConnectorInstallDialog } from "./connector-install-dialog"
import { ConnectorInstalledList } from "./connector-installed-list"
import type { McpMarketplaceItem } from "@/lib/types"
import { useConnectorMarket } from "./use-connector-market"
import { useConnectorInstallTasks } from "./use-connector-install-tasks"
import { ConnectorInstallTasks } from "./connector-install-tasks"
import { connectorTaskActive } from "@/lib/connector-install-tasks"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useMarketPageFocus } from "./use-market-page-focus"
import { ConnectorPublisherView } from "./connector-publisher-view"
import { onTransportChange } from "@/lib/transport"

export function ConnectorMarketView({ action }: { action?: MarketViewAction }) {
  const { profile, status } = useIywAccount()
  const [transportRevision, setTransportRevision] = useState(0)
  useEffect(
    () => onTransportChange(() => setTransportRevision((value) => value + 1)),
    []
  )
  const scope = JSON.stringify([
    status,
    profile?.user_id,
    profile?.org_code,
    profile?.workspace_pending,
    transportRevision,
  ])
  return <ConnectorMarketContent key={scope} action={action} />
}

function ConnectorMarketContent({ action }: { action?: MarketViewAction }) {
  const t = useTranslations("ConnectorMarket")
  const c = useTranslations("CapabilityMarket.catalog")
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
    pageSize,
    automaticPageSize,
    setAutoPageSize,
    setPageSize,
    providers,
    providerId,
    setProviderId,
    reload,
  } = useConnectorMarket(t("unavailable"))
  const [active, setActive] = useState<McpMarketplaceItem | null>(null)
  const [section, setSection] = useState("market")
  const publisherCanLeave = useRef<() => boolean>(() => true)
  const registerPublisherLeave = useCallback((canLeave: () => boolean) => {
    publisherCanLeave.current = canLeave
  }, [])
  const root = useMarketPageFocus({
    page,
    loading,
    error,
    enabled: section === "market",
  })
  useMarketViewAction(action, (kind) => {
    if (kind === "installed" || kind === "market") {
      if (section === "publisher" && !publisherCanLeave.current()) return
      setSection(kind)
    } else setSection("publisher")
  })
  const pagination = {
    automaticPageSize,
    onAutoPageSizeChange: setAutoPageSize,
    page: page + 1,
    pageSize,
    total: null,
    itemCount: items.length,
    hasMore,
    loading,
    unavailable: Boolean(error),
    onPageChange: (value: number) => setPage(value - 1),
    onPageSizeChange: setPageSize,
  }
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
  if (section === "publisher")
    return (
      <ConnectorPublisherView
        action={action}
        onBrowse={() => setSection("market")}
        onLeaveRequest={registerPublisherLeave}
      />
    )
  return (
    <div ref={root} className="@container min-w-0">
      <div className="space-y-4">
        <div className="my-[18px] flex flex-wrap items-center gap-3">
          <div className="relative min-w-[170px] max-w-[330px] flex-1 max-sm:min-w-full max-sm:max-w-none">
            <Search className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="h-9 rounded-md bg-background pl-9 shadow-none max-sm:min-h-11"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("search")}
              aria-label={t("search")}
            />
          </div>
          <select
            aria-label={c("source")}
            value={providerId}
            onChange={(event) => setProviderId(event.target.value)}
            className="h-9 max-w-52 rounded-md border bg-background px-3 text-sm max-sm:min-h-11"
          >
            {providers.map((provider) => (
              <option key={provider.id} value={provider.id}>
                {provider.name}
              </option>
            ))}
          </select>
          <Button
            variant="ghost"
            size="icon-sm"
            disabled={loading}
            aria-label={t("refresh")}
            onClick={() => void reload()}
          >
            <RefreshCw className="size-4" />
          </Button>
          <MarketQuickPages {...pagination} />
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
          {section === "installed" && (
            <Button
              variant="ghost"
              className="w-fit"
              onClick={() => setSection("market")}
            >
              {c("browse")}
            </Button>
          )}
          <TabsContent value="market">
            <div
              data-market-grid
              className={CAPABILITY_GRID_CLASS}
              aria-busy={loading}
            >
              {items.map((item) => (
                <CapabilityCard
                  key={`${item.provider_id}:${item.server_id}`}
                  kind="connector"
                  name={item.name}
                  publisher={item.owner || item.namespace || c("connector")}
                  summary={item.description}
                  category={
                    item.remote ? t("runtime_remote") : t("runtime_local")
                  }
                  source={
                    providers.find(
                      (provider) => provider.id === item.provider_id
                    )?.name || item.provider_id
                  }
                  version={item.latest_version}
                  icon={item.icon_url}
                  installed={installedIds.has(item.server_id)}
                  onOpen={() => setActive(item)}
                />
              ))}
            </div>
            {!error && <MarketPagination {...pagination} />}
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
          <p
            data-market-result-state
            role="status"
            className="py-10 text-center text-sm text-muted-foreground"
          >
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
