"use client"

import { useEffect, useState } from "react"
import {
  mcpListMarketplaces,
  mcpScanLocal,
  mcpSearchMarketplace,
} from "@/lib/api"
import type {
  LocalMcpServer,
  McpMarketplaceItem,
  McpMarketplaceProvider,
} from "@/lib/types"
import {
  DEFAULT_MARKET_PAGE_SIZE,
  marketPageSize,
} from "@/lib/market-pagination"

const SEARCH_DELAY_MS = 300

interface PageResult {
  key: string
  items: McpMarketplaceItem[]
  hasMore: boolean
  error: string
  providers: McpMarketplaceProvider[]
}

export function useConnectorMarket(unavailable: string) {
  const [automaticPageSize, setAutomaticPageSize] = useState(true)
  const [search, setSearch] = useState({
    query: "",
    page: 0,
    pageSize: DEFAULT_MARKET_PAGE_SIZE as number,
    providerId: "",
  })
  const [revision, setRevision] = useState(0)
  const [installed, setInstalled] = useState<LocalMcpServer[]>([])
  const [result, setResult] = useState<PageResult | null>(null)
  const key = JSON.stringify([search, revision])
  useEffect(() => {
    let active = true
    const timer = setTimeout(
      async () => {
        const next: PageResult = {
          key,
          items: [],
          hasMore: false,
          error: "",
          providers: [],
        }
        await loadPage({
          search,
          unavailable,
          next,
          updateInstalled: (items) => {
            if (active) setInstalled(items)
          },
        })
        if (active) setResult(next)
      },
      search.query ? SEARCH_DELAY_MS : 0
    )
    return () => {
      active = false
      clearTimeout(timer)
    }
  }, [key, search, unavailable])
  const current = result?.key === key ? result : null
  return {
    items: current?.items ?? [],
    installed,
    query: search.query,
    page: search.page,
    pageSize: search.pageSize,
    automaticPageSize,
    setAutoPageSize: (size: number) =>
      setSearch((value) =>
        value.pageSize === size
          ? value
          : { ...value, page: 0, pageSize: marketPageSize(size) }
      ),
    providerId: search.providerId || current?.providers[0]?.id || "",
    providers: current?.providers ?? [],
    loading: current == null,
    error: current?.error ?? "",
    hasMore: current?.hasMore ?? false,
    setQuery: (query: string) =>
      setSearch((value) => ({ ...value, query, page: 0 })),
    setPageSize: (size: number) => {
      setAutomaticPageSize(size === 0)
      if (size > 0)
        setSearch((value) => ({
          ...value,
          page: 0,
          pageSize: marketPageSize(size),
        }))
    },
    setProviderId: (providerId: string) =>
      setSearch((value) => ({ ...value, providerId, page: 0 })),
    setPage: (page: number) =>
      setSearch((current) => ({ ...current, page: Math.max(0, page) })),
    reload: async () => {
      setRevision((current) => current + 1)
    },
  }
}

async function loadPage({
  search,
  unavailable,
  next,
  updateInstalled,
}: {
  search: { query: string; page: number; pageSize: number; providerId: string }
  unavailable: string
  next: PageResult
  updateInstalled: (items: LocalMcpServer[]) => void
}) {
  try {
    const [providers, local] = await Promise.all([
      mcpListMarketplaces(),
      mcpScanLocal(),
    ])
    updateInstalled(
      local.filter(
        (item) =>
          item.spec.iyw_builtin == null && item.id !== "iyw-capability-gateway"
      )
    )
    next.providers = providers
    const provider = search.providerId
      ? providers.find((item) => item.id === search.providerId)
      : providers[0]
    if (!provider) throw new Error(unavailable)
    const items: McpMarketplaceItem[] = []
    const needed = search.pageSize + 1
    const batchSize = 50
    while (items.length < needed) {
      const limit = Math.min(batchSize, needed - items.length)
      const batch = await mcpSearchMarketplace({
        providerId: provider.id,
        query: search.query.trim() || null,
        offset: search.page * search.pageSize + items.length,
        limit,
      })
      items.push(...batch)
      if (batch.length < limit) break
    }
    next.items = items.slice(0, search.pageSize)
    next.hasMore = items.length > search.pageSize
  } catch (cause) {
    next.error = String(cause)
  }
}
