"use client"

import { useEffect, useState } from "react"
import { subscribeConversationUsage } from "@/lib/conversation-usage-refresh"
import type { SessionStats } from "@/lib/types"

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
    stats?.total_usage?.confirmed_points,
    stats?.confirmed_consumption,
  ])
}

function preserveUsagePoints(
  stats: SessionStats,
  previous: SessionStats | null
): SessionStats {
  if (
    !stats.confirmed_consumption &&
    previous?.confirmed_consumption &&
    usageSnapshotKey(stats) === usageSnapshotKey(previous)
  ) {
    stats = { ...stats, confirmed_consumption: previous.confirmed_consumption }
  }
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
    return subscribeConversationUsage(conversationId, (snapshot) => {
      if (snapshot.externalId !== sessionId || !snapshot.stats) return
      const stats = snapshot.stats
      setSnapshot((previous) => ({
        key,
        stats: preserveUsagePoints(
          stats,
          previous?.key === key ? previous.stats : null
        ),
        baselineKey,
      }))
    })
  }, [conversationId, sessionId, connectionId, enabled, key, baselineKey])
  if (snapshot?.key !== key) return null
  if (snapshot.baselineKey === baselineKey)
    return preserveUsagePoints(snapshot.stats, baseline)
  return baseline &&
    usageSnapshotKey(snapshot.stats) === usageSnapshotKey(baseline)
    ? preserveUsagePoints(baseline, snapshot.stats)
    : null
}
