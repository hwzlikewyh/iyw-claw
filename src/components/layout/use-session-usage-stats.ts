"use client"

import { useEffect, useState } from "react"
import { getFolderConversation } from "@/lib/api"
import type { SessionStats } from "@/lib/types"

const USAGE_REFRESH_MS = 3_000

interface UsageScope {
  conversationId: number | null
  sessionId: string | null
  connectionId: string | null
  enabled: boolean
}

function usageSnapshotKey(stats: SessionStats | null): string {
  const usage = stats?.total_usage
  return JSON.stringify([
    usage?.input_tokens,
    usage?.output_tokens,
    usage?.cache_read_input_tokens,
    usage?.cache_creation_input_tokens,
    stats?.total_tokens,
  ])
}

function statsSnapshotKey(stats: SessionStats | null): string {
  return JSON.stringify([
    usageSnapshotKey(stats),
    stats?.total_duration_ms,
    stats?.context_window_used_tokens,
    stats?.context_window_max_tokens,
    stats?.context_window_usage_percent,
    stats?.total_usage?.estimated_points,
  ])
}

function preserveUsagePoints(
  stats: SessionStats,
  previous: SessionStats | null
): SessionStats {
  const usage = stats.total_usage
  const points = previous?.total_usage?.estimated_points
  if (
    !usage ||
    usage.estimated_points != null ||
    points == null ||
    usageSnapshotKey(stats) !== usageSnapshotKey(previous)
  )
    return stats
  // 只复用同一份用量的估算，不能把旧点数配给新增 Token。
  return { ...stats, total_usage: { ...usage, estimated_points: points } }
}

function subscribeStats(
  scope: UsageScope,
  onStats: (stats: SessionStats) => void
) {
  let cancelled = false
  let pending = false
  let failed = false
  let timer: ReturnType<typeof setTimeout> | undefined
  const refresh = async () => {
    clearTimeout(timer)
    if (cancelled || pending || document.hidden || !scope.conversationId) return
    pending = true
    try {
      const detail = await getFolderConversation(
        scope.conversationId,
        undefined,
        true
      )
      if (
        !cancelled &&
        detail.summary.external_id === scope.sessionId &&
        detail.session_stats
      ) {
        onStats(detail.session_stats)
      }
      failed = false
    } catch (error) {
      if (!cancelled && !failed)
        console.warn("[session-usage] refresh failed", {
          conversationId: scope.conversationId,
          error,
        })
      failed = true
    } finally {
      pending = false
      if (!cancelled && !document.hidden)
        timer = setTimeout(refresh, USAGE_REFRESH_MS)
    }
  }
  void refresh()
  document.addEventListener("visibilitychange", refresh)
  return () => {
    cancelled = true
    clearTimeout(timer)
    document.removeEventListener("visibilitychange", refresh)
  }
}

export function useSessionUsageStats(
  scope: UsageScope,
  baseline: SessionStats | null
) {
  const { conversationId, sessionId, connectionId, enabled } = scope
  const key = `${conversationId}:${sessionId}:${connectionId}`
  const baselineKey = statsSnapshotKey(baseline)
  const [snapshot, setSnapshot] = useState<{
    key: string
    stats: SessionStats
    baselineKey: string
  } | null>(null)
  useEffect(() => {
    if (!enabled || !conversationId || conversationId <= 0 || !sessionId) return
    return subscribeStats(
      { conversationId, sessionId, connectionId, enabled },
      (stats) =>
        setSnapshot((previous) => ({
          key,
          stats: preserveUsagePoints(
            stats,
            previous?.key === key ? previous.stats : null
          ),
          baselineKey,
        }))
    )
  }, [conversationId, sessionId, connectionId, enabled, key, baselineKey])
  if (snapshot?.key !== key) return null
  if (snapshot.baselineKey === baselineKey)
    return preserveUsagePoints(snapshot.stats, baseline)
  return baseline &&
    usageSnapshotKey(snapshot.stats) === usageSnapshotKey(baseline)
    ? preserveUsagePoints(baseline, snapshot.stats)
    : null
}
