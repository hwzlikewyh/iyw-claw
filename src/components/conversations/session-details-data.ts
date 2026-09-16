import { useEffect, useState } from "react"
import { getFolderConversation } from "@/lib/api"
import type { DbConversationSummary, SessionStats } from "@/lib/types"
import { pickModelFromTurns } from "./active-session-details"

export interface SessionDetailsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  summary: DbConversationSummary
  stats?: SessionStats | null
  model?: string | null
}

interface DetailsResult {
  id: number
  revision: number
  sourceStats: SessionStats | null | undefined
  stats: SessionStats | null
  model: string | null
  summary?: DbConversationSummary
  error: boolean
}

async function fetchDetails(id: number) {
  const detail = await getFolderConversation(id, undefined, true)
  return {
    summary: detail.summary,
    stats: detail.session_stats ?? null,
    model: detail.summary.model ?? pickModelFromTurns(detail.turns),
  }
}

export function useSessionDetails(props: SessionDetailsDialogProps) {
  const [result, setResult] = useState<DetailsResult | null>(null)
  const [revision, refresh] = useState(0)
  const needsFetch = props.stats?.total_usage?.estimated_points == null
  const stats = props.stats
  const id = props.summary.id
  useEffect(() => {
    if (!props.open || !needsFetch) return
    let cancelled = false
    const key = { id, revision, sourceStats: stats }
    void fetchDetails(id)
      .then((detail) => {
        if (cancelled) return
        setResult({
          ...detail,
          ...key,
          error: false,
        })
      })
      .catch(() => {
        if (!cancelled)
          setResult({
            ...key,
            stats: null,
            model: null,
            error: true,
          })
      })
    return () => {
      cancelled = true
    }
  }, [id, needsFetch, props.open, revision, stats])
  const current =
    result?.id === id &&
    result.revision === revision &&
    result.sourceStats === stats
      ? result
      : null
  return {
    summary: current?.summary ?? props.summary,
    stats: needsFetch
      ? (current?.stats ?? props.stats ?? null)
      : (props.stats ?? null),
    model: props.model ?? current?.model ?? props.summary.model ?? null,
    loading: props.open && needsFetch && current === null,
    error: current?.error ?? false,
    retry: () => refresh((value) => value + 1),
  }
}

export function resolveSessionDurationMs(
  summary: DbConversationSummary,
  stats: SessionStats | null
): number {
  if ((stats?.total_duration_ms ?? 0) > 0) return stats!.total_duration_ms
  if (summary.status !== "completed") return 0
  const duration =
    Date.parse(summary.updated_at) - Date.parse(summary.created_at)
  return Number.isFinite(duration) && duration > 0 ? duration : 0
}

const MILLISECONDS_PER_SECOND = 1000
const SECONDS_PER_MINUTE = 60
const MINUTES_PER_HOUR = 60

export function formatSessionDuration(ms: number): string {
  if (ms < MILLISECONDS_PER_SECOND) return `${ms}ms`
  const seconds = ms / MILLISECONDS_PER_SECOND
  if (seconds < SECONDS_PER_MINUTE) return `${Number(seconds.toFixed(1))}s`
  const minutes = seconds / SECONDS_PER_MINUTE
  if (minutes < MINUTES_PER_HOUR) return `${Number(minutes.toFixed(1))}m`
  return `${Number((minutes / MINUTES_PER_HOUR).toFixed(1))}h`
}
