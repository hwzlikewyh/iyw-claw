"use client"

import { useEffect, useState } from "react"
import { getFolderConversation, listAllConversations } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type { DbConversationSummary, SessionStats } from "@/lib/types"

export const CONVERSATIONS_PAGE_SIZE = 10
const LOAD_CONCURRENCY = 3

export interface ConversationUsageRow {
  summary: DbConversationSummary
  stats: SessionStats | null
  error: string | null
}

function activeConversations(rows: DbConversationSummary[], days: number) {
  const start = new Date()
  start.setHours(0, 0, 0, 0)
  start.setDate(start.getDate() - days + 1)
  return rows
    .filter((row) => Date.parse(row.updated_at) >= start.getTime())
    .sort((a, b) => Date.parse(b.updated_at) - Date.parse(a.updated_at))
}

export function useUsageConversationList(days: number) {
  const [result, setResult] = useState<{
    rows: DbConversationSummary[]
    loading: boolean
    error: string | null
  }>({ rows: [], loading: true, error: null })
  useEffect(() => {
    let cancelled = false
    void listAllConversations({ include_children: true })
      .then((rows) => {
        if (!cancelled)
          setResult({
            rows: activeConversations(rows, days),
            loading: false,
            error: null,
          })
      })
      .catch((error) => {
        console.error("[UsageConversations] list failed:", error)
        if (!cancelled)
          setResult({ rows: [], loading: false, error: toErrorMessage(error) })
      })
    return () => {
      cancelled = true
    }
  }, [days])
  return result
}

async function loadRow(
  summary: DbConversationSummary
): Promise<ConversationUsageRow> {
  try {
    let detail = await getFolderConversation(summary.id)
    if (detail.history_stale)
      detail = await getFolderConversation(summary.id, undefined, true)
    return {
      summary: detail.summary,
      stats: detail.session_stats ?? null,
      error: null,
    }
  } catch (error) {
    console.error("[UsageConversations] detail failed:", summary.id, error)
    return { summary, stats: null, error: toErrorMessage(error) }
  }
}

async function loadPage(
  summaries: DbConversationSummary[],
  cancelled: () => boolean
) {
  const rows: ConversationUsageRow[] = []
  for (
    let index = 0;
    index < summaries.length && !cancelled();
    index += LOAD_CONCURRENCY
  ) {
    rows.push(
      ...(await Promise.all(
        summaries.slice(index, index + LOAD_CONCURRENCY).map(loadRow)
      ))
    )
  }
  return rows
}

export function useUsageConversationPage(
  summaries: DbConversationSummary[],
  page: number
) {
  const [result, setResult] = useState<{
    source: DbConversationSummary[]
    page: number
    rows: ConversationUsageRow[]
  } | null>(null)
  useEffect(() => {
    let cancelled = false
    const start = page * CONVERSATIONS_PAGE_SIZE
    void loadPage(
      summaries.slice(start, start + CONVERSATIONS_PAGE_SIZE),
      () => cancelled
    ).then((rows) => {
      if (!cancelled) setResult({ source: summaries, page, rows })
    })
    return () => {
      cancelled = true
    }
  }, [summaries, page])
  return result?.source === summaries && result.page === page
    ? result.rows
    : null
}
