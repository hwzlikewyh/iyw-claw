import { useEffect, useState } from "react"
import {
  connectorUserRequest,
  type OwnedConnectorPage,
} from "@/lib/connector-publishing"
import { extractAppCommandError } from "@/lib/app-error"

const CONTROL_PAGE_SIZE = 50
async function ownedPage(query: {
  page: number
  pageSize: number
  query: string
}): Promise<OwnedConnectorPage> {
  const offset = (query.page - 1) * query.pageSize
  let page = Math.floor(offset / CONTROL_PAGE_SIZE) + 1
  let skip = offset % CONTROL_PAGE_SIZE
  const result: OwnedConnectorPage = {
    items: [],
    total: 0,
    page: query.page,
    pageSize: query.pageSize,
  }
  while (result.items.length < query.pageSize) {
    const batch = await connectorUserRequest<OwnedConnectorPage>("mine", {
      page,
      pageSize: CONTROL_PAGE_SIZE,
      query: query.query,
      status: "all",
    })
    result.total = batch.total
    result.items.push(
      ...batch.items.slice(skip, skip + query.pageSize - result.items.length)
    )
    if (page * CONTROL_PAGE_SIZE >= batch.total || !batch.items.length) break
    page++
    skip = 0
  }
  return result
}

export function useOwnedConnectors() {
  const [query, setQuery] = useState({
    page: 1,
    pageSize: 6,
    query: "",
    revision: 0,
  })
  const [automatic, setAutomatic] = useState(true)
  const [result, setResult] = useState<{
    key: string
    data: OwnedConnectorPage | null
    error: string
  } | null>(null)
  const key = JSON.stringify(query)
  useEffect(() => {
    let active = true
    void ownedPage(query)
      .then((data) => {
        if (active) {
          const lastPage = Math.max(1, Math.ceil(data.total / query.pageSize))
          if (query.page > lastPage)
            setQuery((value) => ({ ...value, page: lastPage }))
          else setResult({ key, data, error: "" })
        }
      })
      .catch((error) => {
        if (active)
          setResult({
            key,
            data: null,
            error: extractAppCommandError(error)?.message || String(error),
          })
      })
    return () => {
      active = false
    }
  }, [key, query])
  const current = result?.key === key ? result : null
  const refresh = () =>
    setQuery((value) => ({ ...value, revision: value.revision + 1 }))
  return {
    items: current?.data?.items ?? [],
    loading: current == null,
    error: current?.error ?? "",
    refresh,
    search: query.query,
    setSearch: (search: string) =>
      setQuery((value) => ({ ...value, query: search, page: 1 })),
    pagination: {
      page: query.page,
      pageSize: query.pageSize,
      total: current?.data?.total ?? 0,
      automaticPageSize: automatic,
      loading: current == null,
      unavailable: Boolean(current?.error),
      onPageChange: (page: number) => setQuery((value) => ({ ...value, page })),
      onPageSizeChange: (size: number) => {
        setAutomatic(size === 0)
        if (size > 0)
          setQuery((value) => ({ ...value, page: 1, pageSize: size }))
      },
      onAutoPageSizeChange: (size: number) =>
        setQuery((value) =>
          value.pageSize === size
            ? value
            : { ...value, pageSize: size, page: 1 }
        ),
    },
  }
}
