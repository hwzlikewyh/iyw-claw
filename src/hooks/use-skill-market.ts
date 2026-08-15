"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useSearchParams } from "next/navigation"
import { toErrorMessage } from "@/lib/app-error"
import type {
  SkillMarketCategory,
  SkillMarketCompatibility,
  SkillMarketDistributionPolicy,
  SkillMarketPublisher,
  SkillMarketSort,
  SkillMarketV2Detail,
  SkillMarketV2FileNode,
  SkillMarketV2Item,
  SkillMarketV2Version,
  SkillMarketViewV2,
} from "@/lib/skill-market"
import {
  getSkillMarketSource,
  type SkillMarketAddVersionRequestV2,
  type SkillMarketMetadataRequestV2,
  type SkillMarketPublishRequestV2,
  type SkillMarketSource,
} from "@/lib/skill-market-source"

const LIST_PAGE_SIZE = 50

// ---------------------------------------------------------------------------
// Lightweight P50/P95 metric collector. Samples are kept per name so the UI
// can answer "list first-content / search response / detail open / action
// ready" without a dependency.
// ---------------------------------------------------------------------------

const metricSamples = new Map<string, number[]>()

export function recordSkillMarketMetric(
  name: string,
  durationMs: number
): void {
  const samples = metricSamples.get(name) ?? []
  samples.push(durationMs)
  if (samples.length > 200) samples.shift()
  metricSamples.set(name, samples)
  if (process.env.NODE_ENV !== "production") {
    console.debug(`[SkillMarketV2] metric ${name}: ${Math.round(durationMs)}ms`)
  }
}

export function getSkillMarketMetricSummary(name: string): {
  samples: number
  p50: number | null
  p95: number | null
} {
  const samples = metricSamples.get(name) ?? []
  if (!samples.length) return { samples: 0, p50: null, p95: null }
  const sorted = samples.slice().sort((left, right) => left - right)
  const p50 =
    sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.5))] ?? null
  const p95 =
    sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))] ??
    null
  return { samples: samples.length, p50, p95 }
}

// ---------------------------------------------------------------------------
// Query-state: view / filters / sort / search live in the URL so returning to
// the page never loses the current market position (static export friendly).
// ---------------------------------------------------------------------------

export interface SkillMarketQueryState {
  view: SkillMarketViewV2
  publisher: SkillMarketPublisher | "all"
  distribution: SkillMarketDistributionPolicy | "all"
  compatibility: SkillMarketCompatibility | "all"
  category: string | null
  sort: SkillMarketSort
  q: string
}

const VIEWS: SkillMarketViewV2[] = [
  "market",
  "organization",
  "mine",
  "installed",
  "needs_update",
]

function isView(value: string | null): value is SkillMarketViewV2 {
  return VIEWS.includes(value as SkillMarketViewV2)
}

function isPublisher(
  value: string | null
): value is SkillMarketPublisher | "all" {
  return value === "all" || value === "official" || value === "user"
}

function isDistribution(
  value: string | null
): value is SkillMarketDistributionPolicy | "all" {
  return value === "all" || value === "mandatory" || value === "optional"
}

function isCompatibility(
  value: string | null
): value is SkillMarketCompatibility | "all" {
  return (
    value === "all" ||
    value === "compatible" ||
    value === "incompatible" ||
    value === "unknown"
  )
}

function isSort(value: string | null): value is SkillMarketSort {
  return (
    value === "recommended" ||
    value === "updated" ||
    value === "name" ||
    value === "installed"
  )
}

function parseQuery(searchParams: URLSearchParams): SkillMarketQueryState {
  return {
    view: isView(searchParams.get("view"))
      ? (searchParams.get("view") as SkillMarketViewV2)
      : "market",
    publisher: isPublisher(searchParams.get("publisher"))
      ? (searchParams.get("publisher") as SkillMarketPublisher | "all")
      : "all",
    distribution: isDistribution(searchParams.get("distribution"))
      ? (searchParams.get("distribution") as
          | SkillMarketDistributionPolicy
          | "all")
      : "all",
    compatibility: isCompatibility(searchParams.get("compatibility"))
      ? (searchParams.get("compatibility") as SkillMarketCompatibility | "all")
      : "all",
    category: searchParams.get("category"),
    sort: isSort(searchParams.get("sort"))
      ? (searchParams.get("sort") as SkillMarketSort)
      : "recommended",
    q: searchParams.get("q") ?? "",
  }
}

function initialQuery(
  searchParams: URLSearchParams,
  targetSkillId?: string | null
): SkillMarketQueryState {
  const query = parseQuery(searchParams)
  if (!targetSkillId) return query
  return {
    ...query,
    view: "market",
    publisher: "all",
    distribution: "all",
    compatibility: "all",
    category: null,
    sort: "recommended",
    q: targetSkillId,
  }
}

function persistQuery(query: SkillMarketQueryState): void {
  if (typeof window === "undefined") return
  const params = new URLSearchParams()
  if (query.view !== "market") params.set("view", query.view)
  if (query.publisher !== "all") params.set("publisher", query.publisher)
  if (query.distribution !== "all")
    params.set("distribution", query.distribution)
  if (query.compatibility !== "all")
    params.set("compatibility", query.compatibility)
  if (query.category) params.set("category", query.category)
  if (query.sort !== "recommended") params.set("sort", query.sort)
  if (query.q) params.set("q", query.q)
  const search = params.toString()
  const url = `${window.location.pathname}${search ? `?${search}` : ""}`
  window.history.replaceState(null, "", url)
}

interface ListState {
  items: SkillMarketV2Item[]
  total: number
  nextCursor: string | null
  loading: boolean
  error: string | null
  revision: string
  offline: boolean
}

const INITIAL_LIST: ListState = {
  items: [],
  total: 0,
  nextCursor: null,
  loading: false,
  error: null,
  revision: "",
  offline: false,
}

export function useSkillMarket(targetSkillId?: string | null) {
  const searchParams = useSearchParams()
  const perfParam = searchParams.get("perf")
  const perfCount = perfParam
    ? Math.min(5000, Math.max(0, Number(perfParam) || 0))
    : undefined

  const [query, setQueryState] = useState<SkillMarketQueryState>(() =>
    initialQuery(searchParams, targetSkillId)
  )
  const updateQuery = useCallback((patch: Partial<SkillMarketQueryState>) => {
    setQueryState((current) => {
      const next = { ...current, ...patch }
      persistQuery(next)
      return next
    })
  }, [])

  const source: SkillMarketSource = useMemo(
    () => getSkillMarketSource(perfCount ? { perfCount } : undefined),
    [perfCount]
  )

  const [list, setList] = useState<ListState>(INITIAL_LIST)
  const requestRef = useRef(0)

  const loadList = useCallback(
    async (cursor: string | null) => {
      const requestId = ++requestRef.current
      const startedAt = performance.now()
      const firstPage = !cursor
      setList((current) => ({ ...current, loading: true, error: null }))
      try {
        const page = await source.list({
          view: query.view,
          publisher: query.publisher,
          distribution: query.distribution,
          compatibility: query.compatibility,
          category: query.category,
          q: query.q,
          sort: query.sort,
          cursor,
          limit: LIST_PAGE_SIZE,
        })
        if (requestId !== requestRef.current) return
        const elapsed = performance.now() - startedAt
        setList((current) => {
          const items = cursor ? [...current.items, ...page.items] : page.items
          return {
            items,
            total: page.total,
            nextCursor: page.nextCursor,
            loading: false,
            error: null,
            revision: page.catalogRevision,
            offline: page.offline,
          }
        })
        if (firstPage) recordSkillMarketMetric("listFirstContent", elapsed)
        if (query.q.trim()) recordSkillMarketMetric("searchResponse", elapsed)
      } catch (error) {
        if (requestId !== requestRef.current) return
        setList((current) => ({
          ...current,
          loading: false,
          error: toErrorMessage(error),
        }))
      }
    },
    [query, source]
  )

  const [refreshKey, setRefreshKey] = useState(0)
  const refresh = useCallback(() => setRefreshKey((current) => current + 1), [])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadList(null)
    }, 300)
    return () => {
      window.clearTimeout(timer)
      requestRef.current += 1
    }
  }, [loadList, refreshKey])

  const loadMore = useCallback(() => {
    if (list.nextCursor) void loadList(list.nextCursor)
  }, [list.nextCursor, loadList])

  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedVersion, setSelectedVersion] = useState<string | null>(null)
  const [detailRetryKey, setDetailRetryKey] = useState(0)
  const retryDetail = useCallback(
    () => setDetailRetryKey((current) => current + 1),
    []
  )

  useEffect(() => {
    if (!list.items.length) return
    if (!selectedId || !list.items.some((item) => item.id === selectedId)) {
      setSelectedId(list.items[0].id)
    }
  }, [list.items, selectedId])

  const selectItem = useCallback((item: SkillMarketV2Item) => {
    setSelectedId(item.id)
    setSelectedVersion(null)
  }, [])

  const detailCache = useRef(new Map<string, SkillMarketV2Detail>())
  const [detail, setDetail] = useState<{
    value: SkillMarketV2Detail | null
    loading: boolean
    error: string | null
  }>({ value: null, loading: false, error: null })

  useEffect(() => {
    if (!selectedId) {
      setDetail({ value: null, loading: false, error: null })
      return
    }
    const key = `${list.revision}:${selectedId}:${selectedVersion ?? ""}`
    const cached = detailCache.current.get(key)
    if (cached) {
      setDetail({ value: cached, loading: false, error: null })
      return
    }
    let cancelled = false
    const startedAt = performance.now()
    setDetail((current) => ({ ...current, loading: true, error: null }))
    void source
      .detail(selectedId, selectedVersion)
      .then((value) => {
        if (cancelled) return
        detailCache.current.set(key, value)
        recordSkillMarketMetric("detailOpen", performance.now() - startedAt)
        setDetail({ value, loading: false, error: null })
      })
      .catch((error) => {
        if (cancelled) return
        setDetail((current) => ({
          ...current,
          loading: false,
          error: toErrorMessage(error),
        }))
      })
    return () => {
      cancelled = true
    }
  }, [detailRetryKey, selectedId, selectedVersion, list.revision, source])

  const [versions, setVersions] = useState<{
    skillId: string | null
    value: SkillMarketV2Version[]
    loading: boolean
    error: string | null
  }>({ skillId: null, value: [], loading: false, error: null })

  useEffect(() => {
    if (!selectedId) {
      setVersions({ skillId: null, value: [], loading: false, error: null })
      return
    }
    let cancelled = false
    setVersions((current) => ({
      ...current,
      skillId: selectedId,
      loading: true,
      error: null,
    }))
    void source
      .versions(selectedId)
      .then((value) => {
        if (cancelled) return
        setVersions({
          skillId: selectedId,
          value,
          loading: false,
          error: null,
        })
      })
      .catch((error) => {
        if (cancelled) return
        setVersions((current) => ({
          ...current,
          loading: false,
          error: toErrorMessage(error),
        }))
      })
    return () => {
      cancelled = true
    }
  }, [selectedId, list.revision, source])

  const filesCache = useRef(new Map<string, SkillMarketV2FileNode[]>())
  const filesRequestRef = useRef(0)
  const [files, setFiles] = useState<{
    key: string | null
    value: SkillMarketV2FileNode[] | null
    loading: boolean
    error: string | null
    requested: boolean
  }>({
    key: null,
    value: null,
    loading: false,
    error: null,
    requested: false,
  })

  const activeVersion =
    selectedVersion ??
    (detail.value?.id === selectedId
      ? detail.value.currentVersion.version
      : null)
  const activeFilesKey =
    selectedId && activeVersion
      ? `${list.revision}:${selectedId}:${activeVersion}`
      : null

  useEffect(() => {
    filesRequestRef.current += 1
    setFiles({
      key: activeFilesKey,
      value: null,
      loading: false,
      error: null,
      requested: false,
    })
  }, [activeFilesKey])

  const openFiles = useCallback(() => {
    if (!selectedId || !activeVersion || !activeFilesKey) return
    const requestId = ++filesRequestRef.current
    const key = activeFilesKey
    const cached = filesCache.current.get(key)
    if (cached) {
      setFiles({
        key,
        value: cached,
        loading: false,
        error: null,
        requested: true,
      })
      return
    }
    setFiles({
      key,
      value: null,
      loading: true,
      error: null,
      requested: true,
    })
    void source
      .files(selectedId, activeVersion)
      .then((value) => {
        if (requestId !== filesRequestRef.current) return
        filesCache.current.set(key, value)
        setFiles({
          key,
          value,
          loading: false,
          error: null,
          requested: true,
        })
      })
      .catch((error) => {
        if (requestId !== filesRequestRef.current) return
        setFiles({
          key,
          value: null,
          loading: false,
          error: toErrorMessage(error),
          requested: true,
        })
      })
  }, [activeFilesKey, activeVersion, selectedId, source])

  const visibleFiles =
    files.key === activeFilesKey
      ? files
      : {
          key: activeFilesKey,
          value: null,
          loading: false,
          error: null,
          requested: false,
        }

  const [categories, setCategories] = useState<SkillMarketCategory[]>([])
  useEffect(() => {
    let cancelled = false
    void source
      .categories()
      .then((value) => {
        if (!cancelled) setCategories(value)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [source])

  const applyItemPatch = useCallback(
    (skillId: string, patch: Partial<SkillMarketV2Item>) => {
      setList((current) => ({
        ...current,
        items: current.items.map((item) =>
          item.id === skillId ? { ...item, ...patch } : item
        ),
      }))
      setDetail((current) =>
        current.value?.id === skillId
          ? { ...current, value: { ...current.value, ...patch } }
          : current
      )
    },
    []
  )

  const applyInstalled = useCallback(
    (skillId: string, installedVersion: string) => {
      applyItemPatch(skillId, {
        installState: "installed",
        installedVersion,
      })
      detailCache.current.clear()
      if (selectedId === skillId) {
        setDetailRetryKey((current) => current + 1)
      }
    },
    [applyItemPatch, selectedId]
  )

  const applyUninstalled = useCallback(
    (skillId: string) => {
      applyItemPatch(skillId, {
        installState: "not_installed",
        installedVersion: null,
      })
      detailCache.current.clear()
      if (selectedId === skillId) {
        setDetailRetryKey((current) => current + 1)
      }
    },
    [applyItemPatch, selectedId]
  )

  const applyArtifactReady = useCallback(
    (skillId: string, rebuilt: SkillMarketV2Version) => {
      setVersions((current) => ({
        ...current,
        value:
          current.skillId === skillId
            ? current.value.map((version) =>
                version.version === rebuilt.version ? rebuilt : version
              )
            : current.value,
      }))
      setList((current) => ({
        ...current,
        items: current.items.map((item) =>
          item.id === skillId && item.currentVersion.version === rebuilt.version
            ? { ...item, currentVersion: rebuilt }
            : item
        ),
      }))
      setDetail((current) =>
        current.value?.id === skillId &&
        current.value.currentVersion.version === rebuilt.version
          ? {
              ...current,
              value: { ...current.value, currentVersion: rebuilt },
            }
          : current
      )
    },
    []
  )

  const applyDeleted = useCallback((skillId: string) => {
    setList((current) => ({
      ...current,
      items: current.items.filter((item) => item.id !== skillId),
    }))
    setSelectedId((current) => (current === skillId ? null : current))
    setDetail({ value: null, loading: false, error: null })
  }, [])

  const publish = useCallback(
    async (request: SkillMarketPublishRequestV2) => {
      const item = await source.publish(request)
      setList((current) => ({
        ...current,
        items: [item, ...current.items],
        total: current.total + 1,
      }))
      setSelectedId(item.id)
      setSelectedVersion(null)
      refresh()
      return item
    },
    [refresh, source]
  )

  const addVersion = useCallback(
    async (request: SkillMarketAddVersionRequestV2) => {
      const item = await source.addVersion(request)
      applyItemPatch(item.id, {
        currentVersion: item.currentVersion,
        installState: item.installState,
        updatedAt: item.updatedAt,
      })
      return item
    },
    [applyItemPatch, source]
  )

  const updateMetadata = useCallback(
    async (request: SkillMarketMetadataRequestV2) => {
      const item = await source.updateMetadata(request)
      applyItemPatch(item.id, {
        displayName: item.displayName,
        summary: item.summary,
        category: item.category,
        iconUrl: item.iconUrl,
        tags: item.tags,
        audience: item.audience,
        updatedAt: item.updatedAt,
      })
      return item
    },
    [applyItemPatch, source]
  )

  const deleteSkill = useCallback(
    async (skillId: string) => {
      await source.delete(skillId)
      applyDeleted(skillId)
      refresh()
    },
    [applyDeleted, refresh, source]
  )

  const uninstallSkill = useCallback(
    async (skillId: string) => {
      await source.uninstall(skillId)
      applyUninstalled(skillId)
      refresh()
    },
    [applyUninstalled, refresh, source]
  )

  const rebuildArtifact = useCallback(
    async (skillId: string, versionValue: string) => {
      const rebuilt = await source.rebuildArtifact(skillId, versionValue)
      applyArtifactReady(skillId, rebuilt)
      return rebuilt
    },
    [applyArtifactReady, source]
  )

  return {
    query,
    updateQuery,
    list,
    loadMore,
    refresh,
    selectedId,
    selectedVersion,
    selectItem,
    selectVersion: setSelectedVersion,
    detail,
    versions,
    files: visibleFiles,
    openFiles,
    retryDetail,
    categories,
    applyInstalled,
    publish,
    addVersion,
    updateMetadata,
    deleteSkill,
    uninstallSkill,
    rebuildArtifact,
  }
}

export type SkillMarketViewModel = ReturnType<typeof useSkillMarket>
