"use client"

import { useEffect, useRef, useState } from "react"

export const OUTPUT_RATE_WINDOW_MS = 5_000
const SAMPLE_INTERVAL_MS = 1_000

interface Sample {
  at: number
  count: number
}

function createSampler(
  readCount: () => number,
  publish: (value: number) => void
) {
  let samples: Sample[] = []
  return () => {
    const now = Date.now()
    const count = readCount()
    const last = samples[samples.length - 1]
    if (!last || count < last.count || now - last.at > OUTPUT_RATE_WINDOW_MS) {
      samples = [{ at: now, count }]
      publish(0)
      return
    }
    samples.push({ at: now, count })
    while (samples.length > 2 && samples[1].at <= now - OUTPUT_RATE_WINDOW_MS)
      samples.shift()
    const first = samples[0]
    const seconds = Math.max(1, (now - first.at) / SAMPLE_INTERVAL_MS)
    publish(Math.round((count - first.count) / seconds))
  }
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
    let timer: ReturnType<typeof setInterval> | undefined
    const publish = (value: number) => {
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
