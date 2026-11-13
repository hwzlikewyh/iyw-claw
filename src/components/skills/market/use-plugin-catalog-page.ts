import { useMemo, useState } from "react"
import {
  DEFAULT_MARKET_PAGE_SIZE,
  marketPage,
  marketPageCount,
  marketPageSize,
} from "@/lib/market-pagination"
import { usePluginCatalog } from "./use-plugin-catalog"

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
  const [search, setSearch] = useState({
    view: "market" as "market" | "organization",
    query: "",
  })
  const [position, setPosition] = useState({
    page: 1,
    pageSize: DEFAULT_MARKET_PAGE_SIZE as number,
  })
  const catalog = usePluginCatalog(search.view)
  const items = useMemo(() => {
    const needle = search.query.trim().toLocaleLowerCase()
    return catalog.items.filter((item) => matchesPluginSearch(item, needle))
  }, [catalog.items, search.query])
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
      page,
      pageSize: position.pageSize,
      total: items.length,
      loading: catalog.loading,
      onPageChange: (page: number) =>
        setPosition((current) => ({ ...current, page: marketPage(page) })),
      onPageSizeChange: (size: number) =>
        setPosition({ page: 1, pageSize: marketPageSize(size) }),
    },
    onViewChange: (view: typeof search.view) => changeSearch({ view }),
    onQueryChange: (query: string) => changeSearch({ query }),
  }
}
