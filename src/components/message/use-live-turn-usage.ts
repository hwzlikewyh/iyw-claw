"use client"

import { useEffect, useState } from "react"
import type { TurnUsage } from "@/lib/types"
import { loadLiveTurnUsage } from "./live-turn-usage"

const USAGE_REFRESH_MS = 3_000
const ELAPSED_REFRESH_MS = 1_000
const MAX_CACHED_TURNS = 1_000
interface UsageSnapshot {
  scope: string
  usage: TurnUsage
  startedAt: number
}
const reportedUsage = new Map<string, UsageSnapshot>()

function rememberUsage(snapshot: UsageSnapshot) {
  reportedUsage.set(snapshot.scope, snapshot)
  if (reportedUsage.size > MAX_CACHED_TURNS) {
    reportedUsage.delete(reportedUsage.keys().next().value!)
  }
}

function subscribeUsage(
  conversationId: number,
  onUsage: (usage: TurnUsage) => void
) {
  let cancelled = false
  let pending = false
  let failed = false
  let timer: ReturnType<typeof setTimeout> | undefined
  const isCurrent = () => !cancelled && !document.hidden
  const refresh = async () => {
    clearTimeout(timer)
    if (!isCurrent() || pending) return
    pending = true
    try {
      const usage = await loadLiveTurnUsage(conversationId, isCurrent)
      if (isCurrent() && usage) onUsage(usage)
      failed = false
    } catch (error) {
      if (!cancelled && !failed)
        console.warn("[live-turn-usage] refresh failed", {
          conversationId,
          error,
        })
      failed = true
    } finally {
      pending = false
      if (isCurrent()) timer = setTimeout(refresh, USAGE_REFRESH_MS)
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

export function useLiveTurnUsage({
  conversationId,
  messageId,
  enabled,
  startedAt,
}: {
  conversationId: number | null
  messageId: string
  enabled: boolean
  startedAt: number | null
}) {
  const scope = `${conversationId}:${messageId}`
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null)
  useEffect(() => {
    if (
      !enabled ||
      conversationId == null ||
      conversationId <= 0 ||
      startedAt == null
    )
      return
    return subscribeUsage(conversationId, (usage) => {
      const next = { scope, usage, startedAt }
      rememberUsage(next)
      setSnapshot(next)
    })
  }, [conversationId, scope, enabled, startedAt])
  const cached = snapshot?.scope === scope ? snapshot : reportedUsage.get(scope)
  return cached?.scope === scope &&
    (startedAt == null || cached.startedAt === startedAt)
    ? cached
    : null
}

export function useLiveTurnDuration(startedAt: number | null) {
  const [now, setNow] = useState(0)
  useEffect(() => {
    if (startedAt == null) return
    let timer: ReturnType<typeof setInterval> | undefined
    const sync = () => {
      clearInterval(timer)
      if (document.hidden) return
      setNow(Date.now())
      timer = setInterval(() => setNow(Date.now()), ELAPSED_REFRESH_MS)
    }
    sync()
    document.addEventListener("visibilitychange", sync)
    return () => {
      clearInterval(timer)
      document.removeEventListener("visibilitychange", sync)
    }
  }, [startedAt])
  return startedAt == null ? null : Math.max(0, now - startedAt)
}
