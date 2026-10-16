"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { automationRunHistory, type AutomationRunQuery } from "@/lib/api"
import { onTransportReconnect, subscribe } from "@/lib/platform"
import type { AutomationRun } from "@/lib/types"

export const RUN_PAGE_SIZE = 50
export const AUTOMATION_CHANGED_EVENT = "automation://changed"

export function useAutomationHistory(query: AutomationRunQuery) {
  const [runs, setRuns] = useState<AutomationRun[]>([])
  const [total, setTotal] = useState(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const request = useRef(0)
  const serialized = JSON.stringify(query)
  const reload = useCallback(async () => {
    const id = ++request.current
    setLoading(true)
    try {
      const result = await automationRunHistory(JSON.parse(serialized))
      if (id !== request.current) return
      setRuns(result.runs)
      setTotal(result.total)
      setError(null)
    } catch (cause) {
      if (id === request.current)
        setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      if (id === request.current) setLoading(false)
    }
  }, [serialized])
  useEffect(() => {
    let cancelled = false
    let unsubscribe: (() => void) | undefined
    void reload()
    void subscribe(AUTOMATION_CHANGED_EVENT, () => void reload()).then(
      (off) => {
        if (cancelled) off()
        else unsubscribe = off
      }
    )
    const reconnect = onTransportReconnect(() => void reload())
    return () => {
      cancelled = true
      request.current += 1
      unsubscribe?.()
      reconnect?.()
    }
  }, [reload])
  return { runs, total, loading, error, reload }
}
