"use client"

import { useEffect, useState } from "react"
import {
  mcpListMarketplaces,
  mcpScanLocal,
  mcpSearchMarketplace,
} from "@/lib/api"
import type { LocalMcpServer, McpMarketplaceItem } from "@/lib/types"

const PAGE_SIZE = 20
const SEARCH_DELAY_MS = 300

interface PageResult {
  key: string
  items: McpMarketplaceItem[]
  hasMore: boolean
  error: string
}

export function useConnectorMarket(unavailable: string) {
  const [search, setSearch] = useState({ query: "", page: 0 })
  const [revision, setRevision] = useState(0)
  const [installed, setInstalled] = useState<LocalMcpServer[]>([])
  const [result, setResult] = useState<PageResult | null>(null)
  const key = JSON.stringify([search.query, search.page, revision])
  useEffect(() => {
    let active = true
    const timer = setTimeout(
      async () => {
        const next: PageResult = { key, items: [], hasMore: false, error: "" }
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
    loading: current == null,
    error: current?.error ?? "",
    hasMore: current?.hasMore ?? false,
    setQuery: (query: string) => setSearch({ query, page: 0 }),
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
  search: { query: string; page: number }
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
    const provider = providers[0]
    if (!provider) throw new Error(unavailable)
    const items = await mcpSearchMarketplace({
      providerId: provider.id,
      query: search.query.trim() || null,
      offset: search.page * PAGE_SIZE,
      limit: PAGE_SIZE + 1,
    })
    next.items = items.slice(0, PAGE_SIZE)
    next.hasMore = items.length > PAGE_SIZE
  } catch (cause) {
    next.error = String(cause)
  }
}
