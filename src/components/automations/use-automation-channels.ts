"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { listChatChannels } from "@/lib/api"
import { onTransportReconnect } from "@/lib/platform"
import type { ChatChannelInfo } from "@/lib/types"

export function useAutomationChannels() {
  const [channels, setChannels] = useState<ChatChannelInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const request = useRef(0)
  const reload = useCallback(async () => {
    const id = ++request.current
    try {
      const result = await listChatChannels()
      if (id !== request.current) return
      setChannels(result)
      setError(null)
    } catch (cause) {
      if (id === request.current)
        setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      if (id === request.current) setLoading(false)
    }
  }, [])
  useEffect(() => {
    void reload()
    const reconnect = onTransportReconnect(() => void reload())
    const focus = () => void reload()
    window.addEventListener("focus", focus)
    return () => {
      request.current += 1
      reconnect?.()
      window.removeEventListener("focus", focus)
    }
  }, [reload])
  return { channels, loading, error, reload }
}
