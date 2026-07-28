"use client"

import { useEffect, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import {
  getInstalledMarketInfo,
  skillMarketCategories,
  skillMarketDetail,
  skillMarketListVersions,
  type SkillMarketCategory,
  type SkillMarketDetail,
  type SkillMarketVersion,
} from "@/lib/skill-market"
import type { AgentSkillItem } from "@/lib/types"

const FALLBACK_CATEGORIES: SkillMarketCategory[] = [
  "office-efficiency",
  "content-creation",
  "dev-programming",
  "data-analysis",
  "design-media",
  "ai-agent",
  "knowledge-management",
  "business-ops",
  "education",
  "professional",
  "it-ops-security",
  "life-service",
].map((key, index) => ({ key, fallbackName: key, sortOrder: index }))

export function useMarketCategories() {
  const [categories, setCategories] =
    useState<SkillMarketCategory[]>(FALLBACK_CATEGORIES)
  useEffect(() => {
    let cancelled = false
    skillMarketCategories()
      .then((next) => {
        if (!cancelled) {
          setCategories([...next].sort((a, b) => a.sortOrder - b.sortOrder))
        }
      })
      .catch(() => {
        if (!cancelled) setCategories(FALLBACK_CATEGORIES)
      })
    return () => {
      cancelled = true
    }
  }, [])
  return categories
}

type RequestRef = { current: number }

export function useMarketDetail({
  selectedId,
  selectedVersion,
  refreshKey,
  request,
}: {
  selectedId: string | null
  selectedVersion: string | null
  refreshKey: number
  request: RequestRef
}) {
  const [detail, setDetail] = useState<SkillMarketDetail | null>(null)
  const [detailLoading, setDetailLoading] = useState(false)
  const [detailError, setDetailError] = useState<string | null>(null)
  const [retryKey, setRetryKey] = useState(0)
  useEffect(() => {
    const requestId = ++request.current
    if (!selectedId) {
      setDetail(null)
      setDetailError(null)
      setDetailLoading(false)
      return
    }
    setDetailLoading(true)
    setDetailError(null)
    setDetail(null)
    skillMarketDetail(selectedId, selectedVersion)
      .then((next) => {
        if (requestId === request.current) setDetail(next)
      })
      .catch((error) => {
        if (requestId !== request.current) return
        setDetail(null)
        setDetailError(toErrorMessage(error))
      })
      .finally(() => {
        if (requestId === request.current) setDetailLoading(false)
      })
  }, [refreshKey, request, retryKey, selectedId, selectedVersion])
  return {
    detail,
    detailLoading,
    detailError,
    retryDetail: () => setRetryKey((current) => current + 1),
  }
}

export function useMarketVersions({
  selectedId,
  refreshKey,
  request,
}: {
  selectedId: string | null
  refreshKey: number
  request: RequestRef
}) {
  const [versions, setVersions] = useState<SkillMarketVersion[]>([])
  const [versionsLoading, setVersionsLoading] = useState(false)
  const [versionsError, setVersionsError] = useState<string | null>(null)
  const [retryKey, setRetryKey] = useState(0)
  useEffect(() => {
    const requestId = ++request.current
    if (!selectedId) {
      setVersions([])
      setVersionsError(null)
      setVersionsLoading(false)
      return
    }
    setVersionsLoading(true)
    setVersionsError(null)
    skillMarketListVersions(selectedId)
      .then((next) => {
        if (requestId === request.current) setVersions(next)
      })
      .catch((error) => {
        if (requestId !== request.current) return
        setVersions([])
        setVersionsError(toErrorMessage(error))
      })
      .finally(() => {
        if (requestId === request.current) setVersionsLoading(false)
      })
  }, [refreshKey, request, retryKey, selectedId])
  return {
    versions,
    versionsLoading,
    versionsError,
    retryVersions: () => setRetryKey((current) => current + 1),
  }
}

export function useInstalledRemoteDetails(
  installedSkills: AgentSkillItem[],
  refreshKey: number
) {
  const [remoteById, setRemoteById] = useState<Map<string, SkillMarketDetail>>(
    new Map()
  )
  useEffect(() => {
    let cancelled = false
    const ids = Array.from(
      new Set(
        installedSkills
          .map((skill) => getInstalledMarketInfo(skill).marketId)
          .filter((id): id is string => Boolean(id))
      )
    )
    Promise.allSettled(ids.map((id) => skillMarketDetail(id))).then(
      (results) => {
        if (cancelled) return
        const next = new Map<string, SkillMarketDetail>()
        results.forEach((result, index) => {
          if (result.status === "fulfilled") next.set(ids[index], result.value)
        })
        setRemoteById(next)
      }
    )
    return () => {
      cancelled = true
    }
  }, [installedSkills, refreshKey])
  return remoteById
}
