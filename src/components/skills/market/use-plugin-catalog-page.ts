import { useMemo, useState } from "react"
import {
  DEFAULT_MARKET_PAGE_SIZE,
  marketPage,
  marketPageCount,
  marketPageSize,
} from "@/lib/market-pagination"
import { usePluginCatalog } from "./use-plugin-catalog"
import type { PluginCatalogView } from "./plugin-market-toolbar"

function matchesPluginSearch(
  item: ReturnType<typeof usePluginCatalog>["items"][number],
  needle: string
) {
  return [
    item.displayName,
    item.summary,
    item.category,
    item.organizationName ?? "",
    ...item.tags,
  ]
    .join(" ")
    .toLocaleLowerCase()
    .includes(needle)
}

export function usePluginCatalogPage() {
  const [automaticPageSize, setAutomaticPageSize] = useState(true)
  const [search, setSearch] = useState({
    view: "all" as PluginCatalogView,
    publisher: "all" as "all" | "official" | "user",
    query: "",
  })
  const [position, setPosition] = useState({
    page: 1,
    pageSize: DEFAULT_MARKET_PAGE_SIZE as number,
  })
  const catalog = usePluginCatalog(search.view)
  const items = useMemo(() => {
    const needle = search.query.trim().toLocaleLowerCase()
    return catalog.items.filter(
      (item) =>
        (search.publisher === "all" || item.publisher === search.publisher) &&
        matchesPluginSearch(item, needle)
    )
  }, [catalog.items, search.query, search.publisher])
  const page = Math.min(
    position.page,
    marketPageCount(items.length, position.pageSize)
  )
  const changeSearch = (patch: Partial<typeof search>) => {
    setSearch((current) => ({ ...current, ...patch }))
    setPosition((current) => ({ ...current, page: 1 }))
  }
  return {
    ...search,
    catalog,
    items: items.slice(
      (page - 1) * position.pageSize,
      page * position.pageSize
    ),
    pagination: {
      automaticPageSize,
      onAutoPageSizeChange: (size: number) =>
        setPosition((current) =>
          current.pageSize === size ? current : { page: 1, pageSize: size }
        ),
      page,
      pageSize: position.pageSize,
      total: items.length,
      loading: catalog.loading,
      unavailable: Boolean(catalog.error),
      onPageChange: (page: number) =>
        setPosition((current) => ({ ...current, page: marketPage(page) })),
      onPageSizeChange: (size: number) => {
        setAutomaticPageSize(size === 0)
        if (size > 0) setPosition({ page: 1, pageSize: marketPageSize(size) })
      },
    },
    onViewChange: (view: typeof search.view) => changeSearch({ view }),
    onPublisherChange: (publisher: typeof search.publisher) =>
      changeSearch({ publisher }),
    onQueryChange: (query: string) => changeSearch({ query }),
  }
}
