"use client"
import { useMemo, useState } from "react"
import { RefreshCw, Search } from "lucide-react"
import { useTranslations } from "next-intl"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { CapabilityCard } from "./capability-card"
import { SkillCard } from "./skill-card"
import { ConnectorInstallDialog } from "./connector-install-dialog"
import { useConnectorInstallTasks } from "./use-connector-install-tasks"
import { MarketPagination } from "./market-pagination"
import { MarketQuickPages } from "./market-quick-pages"
import { marketPageCount, marketPageSize } from "@/lib/market-pagination"
import {
  useCapabilityCatalog,
  type CatalogEntry,
} from "./use-capability-catalog"

export function CapabilityCatalogView() {
  const catalog = useCapabilityCatalog()
  const market = useTranslations("SkillMarketV2")
  const tasks = useConnectorInstallTasks()
  const [active, setActive] = useState<Extract<
    CatalogEntry,
    { kind: "connector" }
  > | null>(null)
  const { search, position, items, page, pagination, providers, change } =
    useCatalogPosition(catalog)
  return (
    <div className="@container">
      <CatalogToolbar
        search={search}
        providers={providers}
        pagination={pagination}
        change={change}
        refresh={catalog.refresh}
      />
      <CatalogBody
        catalog={catalog}
        items={items}
        page={page}
        position={position}
        setActive={setActive}
      />
      {!catalog.loading && !catalog.error && !items.length && (
        <p
          role="status"
          className="py-10 text-center text-sm text-muted-foreground"
        >
          {market("list.empty")}
        </p>
      )}
      {!catalog.error && <MarketPagination {...pagination} />}
      <CatalogInstall
        active={active}
        tasks={tasks}
        onClose={() => setActive(null)}
        refresh={catalog.refresh}
      />
    </div>
  )
}
type ConnectorEntry = Extract<CatalogEntry, { kind: "connector" }>

function useCatalogPosition(catalog: ReturnType<typeof useCapabilityCatalog>) {
  const [search, setSearch] = useState({ query: "", source: "all" })
  const [position, setPosition] = useState({ page: 1, pageSize: 6 })
  const [automaticPageSize, setAutomaticPageSize] = useState(true)
  const items = useMemo(
    () =>
      catalog.items.filter((entry) => {
        const description =
          entry.kind === "connector"
            ? entry.item.description
            : entry.item.summary
        return (
          (search.source === "all" || entry.source === search.source) &&
          (entry.name + " " + description)
            .toLocaleLowerCase()
            .includes(search.query.trim().toLocaleLowerCase())
        )
      }),
    [catalog.items, search]
  )
  const page = Math.min(
    position.page,
    marketPageCount(items.length, position.pageSize)
  )
  const pagination = {
    page,
    pageSize: position.pageSize,
    automaticPageSize,
    onAutoPageSizeChange: (size: number) =>
      setPosition((value) =>
        value.pageSize === size ? value : { page: 1, pageSize: size }
      ),
    total: items.length,
    loading: catalog.loading,
    onPageChange: (next: number) =>
      setPosition((value) => ({ ...value, page: next })),
    onPageSizeChange: (size: number) => {
      setAutomaticPageSize(size === 0)
      if (size > 0) setPosition({ page: 1, pageSize: marketPageSize(size) })
    },
  }
  const providers = catalogProviders(catalog.items)
  const change = (patch: Partial<typeof search>) => {
    setSearch((value) => ({ ...value, ...patch }))
    setPosition((value) => ({ ...value, page: 1 }))
  }
  return { search, position, items, page, pagination, providers, change }
}

type CatalogToolbarProps = Pick<
  ReturnType<typeof useCatalogPosition>,
  "search" | "providers" | "pagination" | "change"
> & { refresh: () => void }

function CatalogToolbar(props: CatalogToolbarProps) {
  const { search, providers, pagination, change, refresh } = props
  const t = useTranslations("CapabilityMarket.catalog"),
    market = useTranslations("SkillMarketV2")
  return (
    <div className="my-[18px] flex flex-wrap items-center gap-3">
      <div className="relative min-w-[170px] max-w-[330px] flex-1 max-sm:min-w-full max-sm:max-w-none">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          className="h-9 rounded-md bg-background pl-9 shadow-none max-sm:min-h-11"
          aria-label={market("search.placeholder")}
          placeholder={market("search.placeholder")}
          value={search.query}
          onChange={(e) => change({ query: e.target.value })}
        />
      </div>
      <select
        className="h-9 rounded-md border bg-background px-3 text-sm max-sm:min-h-11"
        aria-label={t("source")}
        value={search.source}
        onChange={(e) => change({ source: e.target.value })}
      >
        <option value="all">{t("allSources")}</option>
        <option value="official">{t("official")}</option>
        <option value="user">{t("community")}</option>
        <option value="organization">{t("organization")}</option>
        {providers.map(([id, name]) => (
          <option key={id} value={id}>
            {name}
          </option>
        ))}
      </select>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={pagination.loading}
        aria-label={market("refresh.label")}
        onClick={refresh}
      >
        <RefreshCw className="size-4" />
      </Button>
      <MarketQuickPages {...pagination} />
    </div>
  )
}

function CatalogBody({
  catalog,
  items,
  page,
  position,
  setActive,
}: Pick<
  ReturnType<typeof useCatalogPosition>,
  "items" | "page" | "position"
> & {
  catalog: ReturnType<typeof useCapabilityCatalog>
  setActive: (entry: ConnectorEntry) => void
}) {
  const market = useTranslations("SkillMarketV2")
  return (
    <>
      {catalog.error ? (
        <div role="alert" className="py-8">
          <p>{catalog.error}</p>
          <Button variant="outline" onClick={catalog.refresh}>
            {market("list.retry")}
          </Button>
        </div>
      ) : (
        <div
          data-market-grid
          className="grid grid-cols-1 gap-4 @min-[560px]:grid-cols-2 @min-[960px]:grid-cols-3"
        >
          {catalog.loading
            ? Array.from({ length: position.pageSize }, (_, id) => (
                <Skeleton
                  data-market-skeleton
                  key={id}
                  className="h-52 rounded-[10px]"
                />
              ))
            : items
                .slice((page - 1) * position.pageSize, page * position.pageSize)
                .map((entry) => (
                  <CatalogCard
                    key={entry.key}
                    entry={entry}
                    onConnector={setActive}
                  />
                ))}
        </div>
      )}
    </>
  )
}

function CatalogCard({
  entry,
  onConnector,
}: {
  entry: CatalogEntry
  onConnector: (entry: ConnectorEntry) => void
}) {
  const route = useWorkbenchRoute()
  if (entry.kind !== "connector")
    return (
      <SkillCard
        item={entry.item}
        selected={false}
        onSelect={(item) => route.openSkillMarket(item.id, item.packageType)}
        onPrimaryAction={(item) =>
          route.openSkillMarket(item.id, item.packageType)
        }
      />
    )
  return (
    <CapabilityCard
      kind="connector"
      name={entry.name}
      publisher={entry.item.owner || entry.providerName}
      summary={entry.item.description}
      category={entry.item.protocols.join(" / ")}
      source={entry.providerName}
      version={entry.item.latest_version}
      icon={entry.item.icon_url}
      installed={entry.installed}
      onOpen={() => onConnector(entry)}
    />
  )
}

function catalogProviders(items: CatalogEntry[]) {
  return [
    ...new Map(
      items.flatMap((entry) =>
        entry.kind === "connector"
          ? [[entry.source, entry.providerName] as const]
          : []
      )
    ).entries(),
  ]
}

function CatalogInstall({
  active,
  tasks,
  onClose,
  refresh,
}: {
  active: ConnectorEntry | null
  tasks: ReturnType<typeof useConnectorInstallTasks>
  onClose: () => void
  refresh: () => void
}) {
  const { openConnectorMarket } = useWorkbenchRoute()
  if (!active) return null
  return (
    <ConnectorInstallDialog
      key={tasks.scope + active.key}
      item={active.item}
      installed={active.installed}
      task={tasks.tasks.find((task) => task.serverId === active.item.server_id)}
      onClose={onClose}
      onTaskChanged={() => {
        tasks.refresh()
        refresh()
      }}
      onManageInstalled={() => {
        onClose()
        openConnectorMarket()
      }}
    />
  )
}
