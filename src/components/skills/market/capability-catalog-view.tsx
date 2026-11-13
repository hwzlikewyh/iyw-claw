"use client"
import { useMemo, useState } from "react"
import { RefreshCw, Search } from "lucide-react"
import { useTranslations } from "next-intl"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { CapabilityCatalogResults } from "./capability-catalog-results"
import { ConnectorInstallDialog } from "./connector-install-dialog"
import { useConnectorInstallTasks } from "./use-connector-install-tasks"
import { MarketPagination } from "./market-pagination"
import { MarketQuickPages } from "./market-quick-pages"
import { marketPageCount, marketPageSize } from "@/lib/market-pagination"
import {
  useCapabilityCatalog,
  type CatalogEntry,
} from "./use-capability-catalog"
import { useMarketPageFocus } from "./use-market-page-focus"
import { CapabilitySkillDetailDialog } from "./capability-skill-detail-dialog"

export function CapabilityCatalogView() {
  const catalog = useCapabilityCatalog()
  const c = useTranslations("CapabilityMarket.catalog")
  const tasks = useConnectorInstallTasks()
  const [active, setActive] = useState<Extract<
    CatalogEntry,
    { kind: "connector" }
  > | null>(null)
  const [activeSkill, setActiveSkill] = useState<
    Extract<CatalogEntry, { kind: "skill" | "plugin" }>["item"] | null
  >(null)
  const { search, position, items, page, pagination, providers, change } =
    useCatalogPosition(catalog)
  const root = useMarketPageFocus({ ...pagination, error: catalog.error })
  return (
    <div ref={root} className="@container">
      <CatalogToolbar
        search={search}
        providers={providers}
        pagination={pagination}
        change={change}
        refresh={catalog.refresh}
      />
      <CapabilityCatalogResults
        catalog={catalog}
        items={items}
        page={page}
        pageSize={position.pageSize}
        onConnectorSelect={setActive}
        onSkillSelect={setActiveSkill}
      />
      {!catalog.loading && !catalog.error && !items.length && (
        <p
          data-market-result-state
          role="status"
          className="py-10 text-center text-sm text-muted-foreground"
        >
          {c("empty")}
        </p>
      )}
      {!catalog.error && <MarketPagination {...pagination} />}
      <CatalogInstall
        active={active}
        tasks={tasks}
        onClose={() => setActive(null)}
        refresh={catalog.refresh}
      />
      <CapabilitySkillDetailDialog
        item={activeSkill}
        open={Boolean(activeSkill)}
        onClose={() => setActiveSkill(null)}
        onInstalled={catalog.refresh}
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
    () => catalog.items.filter((entry) => matchesCatalogSearch(entry, search)),
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
    unavailable: Boolean(catalog.error),
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

function matchesCatalogSearch(
  entry: CatalogEntry,
  search: { query: string; source: string }
) {
  if (search.source !== "all" && entry.source !== search.source) return false
  const fields =
    entry.kind === "connector"
      ? [entry.name, entry.item.description, entry.providerName]
      : [
          entry.item.displayName,
          entry.item.summary,
          entry.item.category,
          entry.item.organizationName ?? "",
          ...entry.item.tags,
        ]
  return fields
    .join(" ")
    .toLocaleLowerCase()
    .includes(search.query.trim().toLocaleLowerCase())
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
