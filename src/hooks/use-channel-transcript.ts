"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { getFolderConversation } from "@/lib/api"
import { getChannelConversationRuntime } from "@/lib/channel-runtime"
import type { ChannelConversationRuntime } from "@/lib/channel-runtime"
import type { DbConversationDetail, LiveSessionSnapshot } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"
import { useChannelMonitor } from "./use-channel-monitor"

interface TranscriptData {
  runtime: ChannelConversationRuntime
  history: DbConversationDetail | null
  historyError: string | null
  live: LiveSessionSnapshot | null
}

export function useChannelTranscript(
  channelId: number,
  conversationId: number
) {
  const lastLive = useRef<LiveSessionSnapshot | null>(null)
  const lastHistory = useRef<DbConversationDetail | null>(null)
  const requests = useRef(0)
  const request = useCallback(async (): Promise<TranscriptData> => {
    const epoch = ++requests.current
    const runtime = await getChannelConversationRuntime({
      channelId,
      conversationId,
    })
    const live = retainedLive(lastLive.current, runtime.snapshot)
    const result = await readHistory(conversationId, lastHistory.current)
    if (epoch === requests.current) {
      lastLive.current = live
      lastHistory.current = result.history
    }
    return { runtime, ...result, live }
  }, [channelId, conversationId])
  const monitor = useChannelMonitor(request)
  const earlier = useEarlierMessages(
    conversationId,
    monitor.data?.history?.history_start ?? 0
  )
  return { ...monitor, ...earlier }
}

function retainedLive(
  previous: LiveSessionSnapshot | null,
  snapshot: LiveSessionSnapshot | null
) {
  if (snapshot?.live_message) return snapshot
  if (
    snapshot &&
    (snapshot.connection_id !== previous?.connection_id ||
      snapshot.turn_generation !== previous?.turn_generation)
  )
    return null
  return previous
    ? { ...previous, status: snapshot?.status ?? ("disconnected" as const) }
    : null
}

async function readHistory(
  conversationId: number,
  previous: DbConversationDetail | null
) {
  try {
    return {
      history: await getFolderConversation(conversationId),
      historyError: null,
    }
  } catch (error) {
    return { history: previous, historyError: toErrorMessage(error) }
  }
}

function useEarlierMessages(conversationId: number, start: number) {
  const [older, setOlder] = useState<DbConversationDetail | null>(null)
  const [olderLoading, setOlderLoading] = useState(false)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const active = useRef(true)
  useEffect(() => {
    active.current = true
    return () => {
      active.current = false
    }
  }, [])
  const before = older?.history_start ?? start
  const loadOlder = async () => {
    if (!before || olderLoading) return
    setOlderLoading(true)
    try {
      const page = await getFolderConversation(conversationId, before)
      if (active.current) {
        setOlder((previous) => ({
          ...page,
          turns: [...page.turns, ...(previous?.turns ?? [])],
        }))
        setHistoryError(null)
      }
    } catch (error) {
      if (active.current) setHistoryError(toErrorMessage(error))
    } finally {
      if (active.current) setOlderLoading(false)
    }
  }
  return { older, before, loadOlder, olderLoading, historyError }
}
