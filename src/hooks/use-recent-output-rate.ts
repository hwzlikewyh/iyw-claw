"use client"

import { useEffect, useRef, useState } from "react"

export const OUTPUT_RATE_WINDOW_MS = 5_000
const SAMPLE_INTERVAL_MS = 1_000

interface Sample {
  at: number
  count: number
}

export function useRecentOutputRate(key: string | null, count: number) {
  const source = useRef({ key, count })
  const [rate, setRate] = useState<{
    key: string | null
    value: number
  } | null>(null)
  useEffect(() => {
    source.current = { key, count }
  }, [key, count])
  useEffect(() => {
    if (key === null) return
    let samples: Sample[] = [{ at: Date.now(), count: source.current.count }]
    const timer = setInterval(() => {
      if (source.current.key !== key) return
      const now = Date.now()
      const current = source.current.count
      const last = samples[samples.length - 1]
      if (current < last.count || now - last.at > OUTPUT_RATE_WINDOW_MS) {
        samples = [{ at: now, count: current }]
        setRate({ key, value: 0 })
        return
      }
      samples.push({ at: now, count: current })
      while (
        samples.length > 2 &&
        samples[1].at <= now - OUTPUT_RATE_WINDOW_MS
      ) {
        samples.shift()
      }
      const first = samples[0]
      const seconds = Math.max(1, (now - first.at) / SAMPLE_INTERVAL_MS)
      setRate({ key, value: Math.round((current - first.count) / seconds) })
    }, SAMPLE_INTERVAL_MS)
    return () => clearInterval(timer)
  }, [key])
  return rate?.key === key ? rate.value : null
}
