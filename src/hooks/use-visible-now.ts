"use client"

import { useEffect, useState } from "react"

const CLOCK_INTERVAL_MS = 1_000

// 仅刷新可见界面的经过时间，不产生后端轮询或模型请求。
export function useVisibleNow(enabled: boolean) {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    if (!enabled) return
    let timer: ReturnType<typeof setInterval> | undefined
    const sync = () => {
      clearInterval(timer)
      if (document.hidden) return
      setNow(Date.now())
      timer = setInterval(() => setNow(Date.now()), CLOCK_INTERVAL_MS)
    }
    sync()
    document.addEventListener("visibilitychange", sync)
    return () => {
      clearInterval(timer)
      document.removeEventListener("visibilitychange", sync)
    }
  }, [enabled])
  return now
}
