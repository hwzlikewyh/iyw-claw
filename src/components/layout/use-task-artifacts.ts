"use client"

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { listTaskArtifacts, type TaskArtifactInfo } from "@/lib/api"
import { onTransportReconnect, subscribe } from "@/lib/platform"

const REFRESH_DEBOUNCE_MS = 80

interface TaskArtifactFilters {
  conversationId: number | null
  folderId: number | null
  scope: "current" | "all"
}

interface TaskArtifactState {
  items: TaskArtifactInfo[]
  loading: boolean
  refreshing: boolean
  error: boolean
}

const INITIAL_TASK_ARTIFACT_STATE: TaskArtifactState = {
  items: [],
  loading: true,
  refreshing: false,
  error: false,
}

interface ArtifactLoadArgs {
  filters: TaskArtifactFilters
  filterKey: string
  background: boolean
  requestId: number
  requestIdRef: { current: number }
  setState: Dispatch<SetStateAction<TaskArtifactState>>
  loadFailed: string
  trackerRef: { current: ArtifactLoadTracker }
}

interface ArtifactLoadTracker {
  snapshotKey: string | null
  foregroundKey: string | null
  inFlight: Map<string, ArtifactLoadFlight>
}

interface ArtifactLoadFlight {
  promise: Promise<void>
  requestId: number
  refreshQueued: boolean
}

interface ArtifactLoadRequest {
  filters: TaskArtifactFilters
  requestBackground: boolean
  queueIfBusy: boolean
  requestIdRef: { current: number }
  setState: Dispatch<SetStateAction<TaskArtifactState>>
  loadFailed: string
  trackerRef: { current: ArtifactLoadTracker }
}

export function useTaskArtifacts(filters: TaskArtifactFilters) {
  const { state, load, cancel } = useArtifactLoader(filters)
  useInitialArtifactLoad(load, cancel)
  const backgroundRefresh = useCallback(() => void load(true, true), [load])
  useTaskArtifactUpdates(backgroundRefresh)
  const refresh = useCallback(() => load(true, true), [load])
  return { ...state, refresh }
}

function useArtifactLoader(filters: TaskArtifactFilters) {
  const t = useTranslations("Folder.taskArtifacts")
  const loadFailed = t("loadFailed")
  const { conversationId, folderId, scope } = filters
  const [state, setState] = useState(INITIAL_TASK_ARTIFACT_STATE)
  const requestIdRef = useRef(0)
  const trackerRef = useRef<ArtifactLoadTracker>({
    snapshotKey: null,
    foregroundKey: null,
    inFlight: new Map(),
  })
  const load = useCallback(
    (requestBackground = false, queueIfBusy = false) =>
      startArtifactLoad({
        filters: { conversationId, folderId, scope },
        requestBackground,
        queueIfBusy,
        requestIdRef,
        setState,
        loadFailed,
        trackerRef,
      }),
    [conversationId, folderId, loadFailed, scope]
  )
  const cancel = useArtifactCancel(requestIdRef)
  return { state, load, cancel }
}

function startArtifactLoad(request: ArtifactLoadRequest): Promise<void> {
  const { filters, queueIfBusy, requestIdRef, trackerRef } = request
  const filterKey = taskArtifactFilterKey(filters)
  const active = trackerRef.current.inFlight.get(filterKey)
  if (active?.requestId === requestIdRef.current) {
    if (queueIfBusy) active.refreshQueued = true
    return active.promise
  }
  const background = resolveBackgroundLoad(
    request.requestBackground,
    filterKey,
    trackerRef
  )
  const requestId = ++requestIdRef.current
  const rawPromise = performTaskArtifactLoad({
    ...request,
    filterKey,
    background,
    requestId,
  })
  const flight: ArtifactLoadFlight = {
    promise: rawPromise,
    requestId,
    refreshQueued: false,
  }
  flight.promise = rawPromise.finally(() =>
    finishArtifactLoad(request, filterKey, flight)
  )
  trackerRef.current.inFlight.set(filterKey, flight)
  return flight.promise
}

function finishArtifactLoad(
  request: ArtifactLoadRequest,
  filterKey: string,
  flight: ArtifactLoadFlight
) {
  const { inFlight } = request.trackerRef.current
  if (inFlight.get(filterKey) !== flight) return
  inFlight.delete(filterKey)
  if (
    flight.refreshQueued &&
    flight.requestId === request.requestIdRef.current
  ) {
    void startArtifactLoad({
      ...request,
      requestBackground: true,
      queueIfBusy: false,
    })
  }
}

function resolveBackgroundLoad(
  requested: boolean,
  filterKey: string,
  trackerRef: { current: ArtifactLoadTracker }
): boolean {
  const tracker = trackerRef.current
  const background =
    requested &&
    tracker.snapshotKey === filterKey &&
    tracker.foregroundKey !== filterKey
  if (!background) tracker.foregroundKey = filterKey
  return background
}

function useArtifactCancel(requestIdRef: { current: number }) {
  return useCallback(() => void (requestIdRef.current += 1), [requestIdRef])
}

async function performTaskArtifactLoad({
  filters,
  filterKey,
  background,
  requestId,
  requestIdRef,
  setState,
  loadFailed,
  trackerRef,
}: ArtifactLoadArgs): Promise<void> {
  setState((current) => ({
    ...current,
    loading: !background,
    refreshing: background,
    error: false,
  }))
  try {
    const items = await fetchTaskArtifacts(filters)
    if (requestId !== requestIdRef.current) return
    trackerRef.current.snapshotKey = filterKey
    trackerRef.current.foregroundKey = null
    setState({ items, loading: false, refreshing: false, error: false })
  } catch (error) {
    if (requestId !== requestIdRef.current) return
    if (!background) trackerRef.current.foregroundKey = null
    console.error("[task-artifacts] list failed", { filters, error })
    if (background) toast.error(loadFailed)
    setState((current) => ({
      ...current,
      loading: false,
      refreshing: false,
      error: !background,
    }))
  }
}

function taskArtifactFilterKey(filters: TaskArtifactFilters): string {
  const id =
    filters.scope === "current" ? filters.conversationId : filters.folderId
  return `${filters.scope}:${id ?? "none"}`
}

function useInitialArtifactLoad(
  load: (background?: boolean, queueIfBusy?: boolean) => Promise<void>,
  cancel: () => void
) {
  useEffect(() => {
    const timer = setTimeout(() => void load(), 0)
    return () => {
      clearTimeout(timer)
      cancel()
    }
  }, [cancel, load])
}

async function fetchTaskArtifacts(
  filters: TaskArtifactFilters
): Promise<TaskArtifactInfo[]> {
  if (filters.scope === "all" && filters.folderId == null) return []
  if (
    filters.scope === "current" &&
    (filters.conversationId == null || filters.conversationId <= 0)
  ) {
    return []
  }
  return listTaskArtifacts(
    filters.scope === "current"
      ? { conversationId: filters.conversationId }
      : { folderId: filters.folderId }
  )
}

function useTaskArtifactUpdates(refresh: () => void) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const schedule = useCallback(() => {
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      timerRef.current = null
      refresh()
    }, REFRESH_DEBOUNCE_MS)
  }, [refresh])

  useEffect(() => {
    let disposed = false
    let unsubscribe: (() => void) | undefined
    void subscribe("task-artifact://changed", schedule).then((stop) => {
      if (disposed) stop()
      else unsubscribe = stop
    })
    const stopReconnect = onTransportReconnect(schedule)
    return () => {
      disposed = true
      unsubscribe?.()
      stopReconnect?.()
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [schedule])
}
