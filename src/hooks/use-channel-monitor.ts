"use client"

import { useCallback, useEffect, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import { onTransportChange } from "@/lib/transport"

const POLL_INTERVAL_MS = 3_000

// 只观察后端数据；隐藏页面暂停，切换后端或卸载后拒绝旧请求结果。
export function useChannelMonitor<T>(request: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [revision, setRevision] = useState(0)
  const refresh = useCallback(() => setRevision((value) => value + 1), [])
  useEffect(() => {
    let cancelled = false
    let running = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const update = async () => {
      if (running || document.hidden || cancelled) return
      running = true
      clearTimeout(timer)
      try {
        const next = await request()
        if (!cancelled) {
          setData(next)
          setError(null)
        }
      } catch (failure) {
        if (!cancelled) setError(toErrorMessage(failure))
      } finally {
        running = false
        if (!cancelled) {
          setLoading(false)
          timer = setTimeout(() => void update(), POLL_INTERVAL_MS)
        }
      }
    }
    const visible = () => void update()
    void update()
    document.addEventListener("visibilitychange", visible)
    const dispose = onTransportChange(() => {
      cancelled = true
      setData(null)
      setLoading(true)
      refresh()
    })
    return () => {
      cancelled = true
      clearTimeout(timer)
      dispose()
      document.removeEventListener("visibilitychange", visible)
    }
  }, [request, revision, refresh])
  return { data, error, loading, refresh }
}
