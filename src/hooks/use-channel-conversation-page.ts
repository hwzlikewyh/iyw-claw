"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  listChannelConversations,
  ACTIVE_CHANNEL_PHASES,
  WAITING_CHANNEL_PHASES,
} from "@/lib/channel-runtime"
import type {
  ChannelConversationInfo,
  ConversationCursor,
} from "@/lib/channel-runtime"
import { toErrorMessage } from "@/lib/app-error"
import { useChannelMonitor } from "./use-channel-monitor"

export function useChannelConversationPage(channelId: number, search: string) {
  const loaded = useRef(new Map<number, ChannelConversationInfo>())
  const request = useCallback(async () => {
    const page = await listChannelConversations({ channelId, search })
    page.items.forEach((item) => loaded.current.set(item.conversationId, item))
    return { ...page, items: Array.from(loaded.current.values()) }
  }, [channelId, search])
  const monitor = useChannelMonitor(request)
  const more = useMoreConversations({
    channelId,
    search,
    firstCursor: monitor.data?.nextBefore,
  })
  const items = Array.from(
    new Map(
      [...more.older, ...(monitor.data?.items ?? [])].map((item) => [
        item.conversationId,
        item,
      ])
    ).values()
  )
  if (monitor.error)
    items.forEach((item, index) => {
      items[index] = { ...item, phase: "unknown" }
    })
  items.sort(
    (a, b) =>
      Number(isActive(b)) - Number(isActive(a)) ||
      Date.parse(b.updatedAt) - Date.parse(a.updatedAt) ||
      b.conversationId - a.conversationId
  )
  return { ...monitor, ...more, items, error: monitor.error ?? more.moreError }
}

function isActive(item: ChannelConversationInfo) {
  return (
    ACTIVE_CHANNEL_PHASES.has(item.phase) ||
    WAITING_CHANNEL_PHASES.has(item.phase)
  )
}

function useMoreConversations({
  channelId,
  search,
  firstCursor,
}: {
  channelId: number
  search: string
  firstCursor?: ConversationCursor | null
}) {
  const [older, setOlder] = useState<ChannelConversationInfo[]>([])
  const [cursor, setCursor] = useState<ConversationCursor | null | undefined>()
  const [moreLoading, setMoreLoading] = useState(false)
  const [moreError, setMoreError] = useState<string | null>(null)
  const active = useRef(true)
  useEffect(() => {
    active.current = true
    return () => {
      active.current = false
    }
  }, [])
  const before = cursor === undefined ? firstCursor : cursor
  const loadMore = async () => {
    if (!before || moreLoading) return
    setMoreLoading(true)
    try {
      const page = await listChannelConversations({ channelId, search, before })
      if (active.current) {
        setOlder((current) => [...current, ...page.items])
        setCursor(page.nextBefore)
        setMoreError(null)
      }
    } catch (error) {
      if (active.current) setMoreError(toErrorMessage(error))
    } finally {
      if (active.current) setMoreLoading(false)
    }
  }
  return { older, before, loadMore, moreLoading, moreError }
}
