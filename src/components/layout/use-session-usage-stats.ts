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
  const [snapshot, setSnapshot] = useState<{
    key: string
    stats: SessionStats
    baseline: SessionStats | null
  } | null>(null)
  useEffect(() => {
    if (!enabled || !conversationId || conversationId <= 0 || !sessionId) return
    return subscribeStats(
      { conversationId, sessionId, connectionId, enabled },
      (stats) => setSnapshot({ key, stats, baseline })
    )
  }, [conversationId, sessionId, connectionId, enabled, key, baseline])
  return snapshot?.key === key && snapshot.baseline === baseline
    ? snapshot.stats
    : null
}
