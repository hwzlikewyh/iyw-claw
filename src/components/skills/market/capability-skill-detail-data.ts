import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import type {
  SkillMarketV2Detail,
  SkillMarketV2FileNode,
  SkillMarketV2Item,
  SkillMarketV2Version,
} from "@/lib/skill-market"
import { getSkillMarketSource } from "@/lib/skill-market-source"

export interface DetailState {
  itemId: string | null
  detail: SkillMarketV2Detail | null
  versions: SkillMarketV2Version[]
  versionsLoading: boolean
  selectedVersion: string | null
  loading: boolean
  error: string | null
  files: {
    value: SkillMarketV2FileNode[] | null
    loading: boolean
    error: string | null
    requested: boolean
  }
}

export const EMPTY_FILES = {
  value: null,
  loading: false,
  error: null,
  requested: false,
}

const EMPTY_STATE: DetailState = {
  itemId: null,
  detail: null,
  versions: [],
  versionsLoading: false,
  selectedVersion: null,
  loading: false,
  error: null,
  files: EMPTY_FILES,
}

export function itemAsDetail(item: SkillMarketV2Item): SkillMarketV2Detail {
  return {
    ...item,
    files: [],
    installTargets: [],
    ownership: { source: "market", managed: item.canManage },
    compatibilityDetail: {
      minClientVersion: null,
      osArch: null,
      reason: null,
      deadline: null,
    },
  }
}

function stateForItem(current: DetailState, itemId: string): DetailState {
  return current.itemId === itemId
    ? current
    : { ...EMPTY_STATE, itemId, loading: true, versionsLoading: true }
}

export function markInstalled(
  detail: SkillMarketV2Detail,
  version: string
): SkillMarketV2Detail {
  return {
    ...detail,
    installState:
      detail.currentVersion.version === version
        ? "installed"
        : "update_available",
    installedVersion: version,
  }
}

function useDetailRequest(
  item: SkillMarketV2Item | null,
  revision: number,
  source: ReturnType<typeof getSkillMarketSource>,
  setState: React.Dispatch<React.SetStateAction<DetailState>>
) {
  useEffect(() => {
    if (!item) return
    let active = true
    void source
      .detail(item.id)
      .then((detail) => {
        if (active)
          setState((current) => ({
            ...stateForItem(current, item.id),
            detail,
            loading: false,
            error: null,
          }))
      })
      .catch((cause: unknown) => {
        if (active)
          setState((current) => ({
            ...stateForItem(current, item.id),
            loading: false,
            error: toErrorMessage(cause),
          }))
      })
    return () => {
      active = false
    }
  }, [item, revision, setState, source])
}

function useVersionsRequest(
  item: SkillMarketV2Item | null,
  revision: number,
  source: ReturnType<typeof getSkillMarketSource>,
  setState: React.Dispatch<React.SetStateAction<DetailState>>
) {
  useEffect(() => {
    if (!item) return
    let active = true
    void source
      .versions(item.id)
      .then((versions) => {
        if (active)
          setState((current) => ({
            ...stateForItem(current, item.id),
            versions,
            versionsLoading: false,
          }))
      })
      .catch(() => {
        if (active)
          setState((current) => ({
            ...stateForItem(current, item.id),
            versionsLoading: false,
          }))
      })
    return () => {
      active = false
    }
  }, [item, revision, setState, source])
}

function setFilesLoading(
  setState: React.Dispatch<React.SetStateAction<DetailState>>
) {
  setState((current) => ({
    ...current,
    files: { value: null, loading: true, error: null, requested: true },
  }))
}

function useDetailFiles(
  item: SkillMarketV2Item | null,
  detail: SkillMarketV2Detail | null,
  state: DetailState,
  setState: React.Dispatch<React.SetStateAction<DetailState>>,
  source: ReturnType<typeof getSkillMarketSource>
) {
  const requestRef = useRef(0)
  useEffect(() => {
    requestRef.current += 1
  }, [item?.id])
  const openFiles = useCallback(() => {
    if (!item || !detail) return
    const request = ++requestRef.current
    const version = state.selectedVersion ?? detail.currentVersion.version
    setFilesLoading(setState)
    void source
      .files(item.id, version)
      .then((files) => {
        if (request === requestRef.current)
          setState((current) => ({
            ...current,
            files: {
              value: files,
              loading: false,
              error: null,
              requested: true,
            },
          }))
      })
      .catch((cause: unknown) => {
        if (request === requestRef.current)
          setState((current) => ({
            ...current,
            files: {
              value: null,
              loading: false,
              error: toErrorMessage(cause),
              requested: true,
            },
          }))
      })
  }, [detail, item, setState, source, state.selectedVersion])
  const selectVersion = useCallback(
    (version: string) => {
      requestRef.current += 1
      setState((current) => ({
        ...current,
        selectedVersion: version,
        files: EMPTY_FILES,
      }))
    },
    [setState]
  )
  return {
    openFiles,
    selectVersion,
  }
}

export function useDetailData(item: SkillMarketV2Item | null) {
  const source = useMemo(() => getSkillMarketSource(), [])
  const [revision, setRevision] = useState(0)
  const [state, setState] = useState(EMPTY_STATE)
  useDetailRequest(item, revision, source, setState)
  useVersionsRequest(item, revision, source, setState)
  const current = Boolean(item && state.itemId === item.id)
  const detail = current ? state.detail : item ? itemAsDetail(item) : null
  const files = useDetailFiles(item, detail, state, setState, source)
  return {
    source,
    state,
    setState,
    detail,
    current,
    retry: () => setRevision((value) => value + 1),
    ...files,
  }
}
