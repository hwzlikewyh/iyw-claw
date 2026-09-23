"use client"

import { useEffect, useRef, useState } from "react"

export const OUTPUT_RATE_WINDOW_MS = 1_000
const SAMPLE_INTERVAL_MS = 250

interface Sample {
  at: number
  count: number
}

function createSampler(
  readCount: () => number,
  publish: (value: number | null) => void
) {
  let samples: Sample[] = []
  return () => {
    const now = performance.now()
    const count = readCount()
    if (!Number.isFinite(count) || count < 0) {
      samples = []
      publish(null)
      return
    }

    const last = samples[samples.length - 1]
    if (last && count < last.count) samples = []
    samples.push({ at: now, count })
    while (samples.length > 1 && samples[1].at <= now - OUTPUT_RATE_WINDOW_MS)
      samples.shift()

    const first = samples[0]
    const elapsedMs = now - first.at
    if (elapsedMs < OUTPUT_RATE_WINDOW_MS) {
      publish(null)
      return
    }
    publish(((count - first.count) * 1_000) / elapsedMs)
  }
}

export function useRecentOutputRate(key: string | null, count: number) {
  const source = useRef({ key, count })
  const [rate, setRate] = useState<{
    key: string | null
    value: number | null
  } | null>(null)
  useEffect(() => {
    source.current = { key, count }
  }, [key, count])
  useEffect(() => {
    if (key === null) return
    let timer: ReturnType<typeof setInterval> | undefined
    const publish = (value: number | null) => {
      if (source.current.key !== key) return
      setRate((previous) =>
        previous?.key === key && previous.value === value
          ? previous
          : { key, value }
      )
    }
    const sync = () => {
      clearInterval(timer)
      if (document.hidden) return
      const sample = createSampler(() => source.current.count, publish)
      sample()
      timer = setInterval(sample, SAMPLE_INTERVAL_MS)
    }
    sync()
    document.addEventListener("visibilitychange", sync)
    return () => {
      clearInterval(timer)
      document.removeEventListener("visibilitychange", sync)
    }
  }, [key])
  return key !== null && rate?.key === key ? rate.value : null
}
