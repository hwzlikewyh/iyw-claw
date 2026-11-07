import { useCallback, useEffect, useRef, useState } from "react"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import type { SkillMarketSource } from "@/lib/skill-market-source"
import type { SkillMarketQueryState } from "./use-skill-market"
import { toErrorMessage } from "@/lib/app-error"
import {
  marketPage,
  marketPageSize,
  marketPageCount,
  persistMarketPage,
} from "@/lib/market-pagination"

const SEARCH_DELAY_MS = 300
const INITIAL_LIST = {
  items: [] as SkillMarketV2Item[],
  total: 0,
  nextCursor: null as string | null,
  loading: true,
  error: null as string | null,
  revision: "",
  offline: false,
}

export function useSkillMarketList(input: {
  packageType: "skill" | "plugin"
  query: SkillMarketQueryState
  source: SkillMarketSource
  initial: { page: string | null; pageSize: string | null; persist?: boolean }
  metric: (name: string, elapsed: number) => void
}) {
  const { query, source, metric, packageType } = input
  const [list, setList] = useState(INITIAL_LIST)
  const requestRef = useRef({ value: 0 })
  const {
    position,
    setPosition,
    revision,
    refresh,
    reset,
    move,
    automatic,
    chooseSize,
    autoSize,
  } = useListControls(input.initial, requestRef, setList)
  useCatalogRequest({
    packageType,
    query,
    source,
    metric,
    position,
    revision,
    requestRef,
    setList,
    setPosition,
  })
  return {
    list,
    setList,
    refresh,
    reset,
    pagination: {
      ...position,
      onPageChange: (page: number) => move(page, position.pageSize),
      onPageSizeChange: chooseSize,
      automaticPageSize: automatic,
      onAutoPageSizeChange: autoSize,
    },
  }
}

function useListControls(
  initial: { page: string | null; pageSize: string | null; persist?: boolean },
  requestRef: React.RefObject<{ value: number }>,
  setList: React.Dispatch<React.SetStateAction<typeof INITIAL_LIST>>
) {
  const [automatic, setAutomatic] = useState(
    () => !initial.pageSize || initial.pageSize === "auto"
  )
  const [position, setPosition] = useState(() => ({
    page: marketPage(initial.page),
    pageSize: marketPageSize(initial.pageSize),
  }))
  const [revision, setRevision] = useState(0)
  const refresh = useCallback(() => {
    invalidateList(requestRef, setList)
    setRevision((value) => value + 1)
  }, [requestRef, setList])
  const move = useCallback(
    (page: number, pageSize: number) => {
      invalidateList(requestRef, setList)
      setPosition({
        page: marketPage(page),
        pageSize: marketPageSize(pageSize),
      })
    },
    [requestRef, setList]
  )
  const reset = useCallback(() => {
    ++requestRef.current.value
    setList(INITIAL_LIST)
    setPosition((value) => ({ ...value, page: 1 }))
  }, [requestRef, setList])
  const chooseSize = (size: number) => {
    setAutomatic(size === 0)
    if (size > 0) move(1, size)
  }
  const autoSize = useCallback((size: number) => move(1, size), [move])
  useEffect(() => {
    if (initial.persist !== false)
      persistMarketPage(position.page, position.pageSize, automatic)
  }, [position, initial.persist, automatic])
  return {
    position,
    setPosition,
    revision,
    refresh,
    reset,
    move,
    automatic,
    chooseSize,
    autoSize,
  }
}

function invalidateList(
  requestRef: React.RefObject<{ value: number }>,
  setList: React.Dispatch<React.SetStateAction<typeof INITIAL_LIST>>
) {
  ++requestRef.current.value
  setList((current) => ({ ...current, items: [], loading: true, error: null }))
}

type RequestInput = Pick<
  Parameters<typeof useSkillMarketList>[0],
  "query" | "source" | "metric" | "packageType"
> & {
  position: { page: number; pageSize: number }
  revision: number
  requestRef: React.RefObject<{ value: number }>
  setList: React.Dispatch<React.SetStateAction<typeof INITIAL_LIST>>
  setPosition: React.Dispatch<
    React.SetStateAction<{ page: number; pageSize: number }>
  >
}

function useCatalogRequest(input: RequestInput) {
  const {
    query,
    source,
    metric,
    position,
    revision,
    requestRef,
    setList,
    setPosition,
    packageType,
  } = input
  useEffect(() => {
    const counter = requestRef.current
    const request = ++counter.value
    const timer = setTimeout(
      () =>
        void readPage(
          {
            packageType,
            query,
            source,
            metric,
            position,
            revision,
            requestRef,
            setList,
            setPosition,
          },
          request
        ),
      query.q ? SEARCH_DELAY_MS : 0
    )
    return () => {
      clearTimeout(timer)
      ++counter.value
    }
  }, [
    query,
    source,
    metric,
    position,
    revision,
    requestRef,
    setList,
    setPosition,
    packageType,
  ])
}

async function readPage(input: RequestInput, request: number) {
  const { query, source, metric, position, requestRef, setList, setPosition } =
    input
  const counter = requestRef.current

  const startedAt = performance.now()
  try {
    const value = await source.list({
      ...query,
      packageType: input.packageType,
      cursor:
        position.page === 1
          ? null
          : String((position.page - 1) * position.pageSize),
      limit: position.pageSize,
    })
    if (counter.value !== request) return
    const last = marketPageCount(value.total, position.pageSize)
    if (position.page > last) {
      setPosition((current) => ({ ...current, page: last }))
      return
    }
    setList({
      items: value.items,
      total: value.total,
      nextCursor: value.nextCursor,
      loading: false,
      error: null,
      revision: value.catalogRevision,
      offline: value.offline,
    })
    if (position.page === 1)
      metric("listFirstContent", performance.now() - startedAt)
    if (query.q.trim()) metric("searchResponse", performance.now() - startedAt)
  } catch (cause) {
    if (counter.value === request)
      setList((current) => ({
        ...current,
        items: [],
        loading: false,
        error: toErrorMessage(cause),
      }))
  }
}
