"use client"

import {
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import type { SkillMarketSection } from "@/components/skills/skill-market-toolbar"
import { toErrorMessage } from "@/lib/app-error"
import {
  skillMarketList,
  type SkillMarketItem,
  type SkillMarketPublisher,
  type SkillMarketVisibility,
} from "@/lib/skill-market"

const MARKET_PAGE_SIZE = 20

export function useMarketFilters() {
  const [query, setQuery] = useState("")
  const [category, setCategory] = useState<string | null>(null)
  const [publisher, setPublisher] = useState<SkillMarketPublisher | "all">(
    "all"
  )
  const [visibility, setVisibility] = useState<SkillMarketVisibility | "all">(
    "all"
  )
  return useMemo(
    () => ({
      query,
      category,
      publisher,
      visibility,
      setQuery,
      setCategory,
      setPublisher,
      setVisibility,
    }),
    [category, publisher, query, visibility]
  )
}

type Filters = ReturnType<typeof useMarketFilters>
type SetSelected = (value: string | null) => void

type ListRequest = {
  request: MutableRefObject<number>
  section: SkillMarketSection
  filters: Filters
  page: number
  pageSize: number
  setItems: Dispatch<SetStateAction<SkillMarketItem[]>>
  setLoading: Dispatch<SetStateAction<boolean>>
  setError: Dispatch<SetStateAction<string | null>>
  setPage: Dispatch<SetStateAction<number>>
  setPageSize: Dispatch<SetStateAction<number>>
  setTotal: Dispatch<SetStateAction<number>>
  getSelectedId: () => string | null
  setSelectedId: SetSelected
  setSelectedVersion: SetSelected
}

function normalizePagination(result: {
  total: number
  page: number
  pageSize: number
}) {
  const total = Math.max(0, result.total)
  const pageSize = Math.max(1, result.pageSize)
  const totalPages = Math.max(1, Math.ceil(total / pageSize))
  const page = Math.min(Math.max(1, result.page), totalPages)
  return { total, page, pageSize }
}

function applyListResult(
  context: ListRequest,
  result: Awaited<ReturnType<typeof skillMarketList>>
) {
  const pagination = normalizePagination(result)
  context.setPageSize(pagination.pageSize)
  context.setTotal(pagination.total)
  context.setPage(pagination.page)
  context.setSelectedVersion(null)
  if (pagination.page !== result.page) {
    context.setItems([])
    context.setSelectedId(null)
    return
  }
  context.setItems(result.items)
  const current = context.getSelectedId()
  context.setSelectedId(
    current && result.items.some((item) => item.id === current)
      ? current
      : (result.items[0]?.id ?? null)
  )
}

async function requestList(context: ListRequest) {
  if (context.section === "installed") {
    context.request.current += 1
    context.setLoading(false)
    return
  }
  const requestId = ++context.request.current
  context.setLoading(true)
  context.setError(null)
  try {
    const result = await skillMarketList({
      view: context.section,
      visibility:
        context.section === "mine" ? context.filters.visibility : "all",
      publisherType: context.filters.publisher,
      category: context.filters.category,
      q: context.filters.query.trim(),
      page: context.page,
      pageSize: context.pageSize,
    })
    if (requestId === context.request.current) applyListResult(context, result)
  } catch (error) {
    if (requestId !== context.request.current) return
    context.setItems([])
    context.setSelectedId(null)
    context.setTotal(0)
    context.setError(toErrorMessage(error))
  } finally {
    if (requestId === context.request.current) context.setLoading(false)
  }
}

function useListingState() {
  const [items, setItems] = useState<SkillMarketItem[]>([])
  const [listLoading, setListLoading] = useState(false)
  const [listError, setListError] = useState<string | null>(null)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(MARKET_PAGE_SIZE)
  const [total, setTotal] = useState(0)
  const [refreshKey, setRefreshKey] = useState(0)
  return {
    items,
    listLoading,
    listError,
    page,
    pageSize,
    total,
    refreshKey,
    setItems,
    setListLoading,
    setListError,
    setPage,
    setPageSize,
    setTotal,
    setRefreshKey,
  }
}

type ListingState = ReturnType<typeof useListingState>

type ListingOptions = {
  section: SkillMarketSection
  filters: Filters
  getSelectedId: () => string | null
  setSelectedId: SetSelected
  setSelectedVersion: SetSelected
}

function useListLoader({
  section,
  filters,
  state,
  getSelectedId,
  setSelectedId,
  setSelectedVersion,
}: ListingOptions & { state: ListingState }) {
  const request = useRef(0)
  const load = useCallback(
    () =>
      requestList({
        request,
        section,
        filters,
        page: state.page,
        pageSize: state.pageSize,
        setItems: state.setItems,
        setLoading: state.setListLoading,
        setError: state.setListError,
        setPage: state.setPage,
        setPageSize: state.setPageSize,
        setTotal: state.setTotal,
        getSelectedId,
        setSelectedId,
        setSelectedVersion,
      }),
    [
      filters,
      getSelectedId,
      section,
      setSelectedId,
      setSelectedVersion,
      state.page,
      state.pageSize,
      state.setItems,
      state.setListError,
      state.setListLoading,
      state.setPage,
      state.setPageSize,
      state.setTotal,
    ]
  )
  const cancel = useCallback(() => {
    request.current += 1
  }, [])
  return { load, cancel }
}

export function useMarketListing(options: ListingOptions) {
  const listing = useListingState()
  const { load, cancel } = useListLoader({ ...options, state: listing })
  useEffect(
    () => listing.setPage(1),
    [listing.setPage, options.filters, options.section]
  )
  useEffect(() => {
    if (options.section === "installed") {
      void load()
      return cancel
    }
    const timeout = window.setTimeout(() => void load(), 250)
    return () => {
      window.clearTimeout(timeout)
      cancel()
    }
  }, [cancel, load, listing.refreshKey, options.section])
  return {
    refreshKey: listing.refreshKey,
    state: {
      items: listing.items,
      listLoading: listing.listLoading,
      listError: listing.listError,
      page: listing.page,
      pageSize: listing.pageSize,
      total: listing.total,
      setPage: listing.setPage,
      refresh: () => listing.setRefreshKey((current) => current + 1),
    },
  }
}
