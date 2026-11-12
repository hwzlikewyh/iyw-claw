"use client"

import { useEffect, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"
import {
  inspectPluginRecovery,
  applyPluginRecovery,
  type PluginRecoveryReview,
} from "@/lib/plugin-recovery"

export function usePluginRecovery(id: string, onRecovered: () => void) {
  const [transport] = useState(getTransport)
  const [view, setView] = useState<PluginRecoveryReview | null>(null),
    [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const alive = useRef(true),
    running = useRef(false)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])
  const run = async (apply: boolean) => {
    if (running.current || (apply && !view?.reviewToken)) return
    running.current = true
    setBusy(true)
    setError("")
    try {
      const result = apply
        ? await applyPluginRecovery(id, view!.reviewToken!, transport)
        : await inspectPluginRecovery(id, transport)
      if (alive.current) {
        setView(result)
        if (result.state === "recovered") onRecovered()
      }
    } catch (cause) {
      if (alive.current)
        setError(extractAppCommandError(cause)?.message || String(cause))
    } finally {
      running.current = false
      if (alive.current) setBusy(false)
    }
  }
  return {
    view,
    error,
    busy,
    inspect: () => run(false),
    apply: () => run(true),
  }
}
