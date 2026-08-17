"use client"

import { useEffect, useMemo, useState } from "react"
import { browserApi } from "@/lib/browser-api"
import { getCurrentWindow, isDesktop } from "@/lib/platform"
import { useBrowser } from "@/contexts/browser-context"

const HEARTBEAT_MS = 2_000
let hostTransition = Promise.resolve()

export function useBrowserHost(kind: "docked" | "detached", enabled: boolean) {
  const { state, acceptState, refresh } = useBrowser()
  const [windowLabel, setWindowLabel] = useState<string | null>(null)
  const [hostId, setHostId] = useState<string | null>(null)

  useEffect(() => {
    if (!enabled || !isDesktop()) return
    let disposed = false
    let registeredHostId: string | null = null
    queueHostTransition(async () => {
      const window = await getCurrentWindow()
      if (!window || disposed) return
      setWindowLabel(window.label)
      const registration = await browserApi.registerHost(window.label, kind)
      if (disposed) {
        await browserApi.unregisterHost(registration.hostId).catch(() => {})
      } else {
        registeredHostId = registration.hostId
        setHostId(registration.hostId)
        acceptState(registration.state)
      }
    })
    return () => {
      disposed = true
      setHostId(null)
      queueHostTransition(async () => {
        if (disposed) {
          const hostId = registeredHostId
          registeredHostId = null
          if (hostId) await browserApi.unregisterHost(hostId).catch(() => {})
        }
      })
    }
  }, [acceptState, enabled, kind])

  const host = useMemo(
    () => state?.hosts.find((item) => item.hostId === hostId) ?? null,
    [hostId, state?.hosts]
  )
  const hostGeneration = host?.generation

  useEffect(() => {
    if (!hostId || hostGeneration === undefined || !enabled) return
    const heartbeat = () => {
      void browserApi
        .heartbeatHost(hostId, hostGeneration, !document.hidden)
        .then(acceptState)
        .catch(() => void refresh())
    }
    const timer = window.setInterval(heartbeat, HEARTBEAT_MS)
    document.addEventListener("visibilitychange", heartbeat)
    heartbeat()
    return () => {
      window.clearInterval(timer)
      document.removeEventListener("visibilitychange", heartbeat)
    }
  }, [acceptState, enabled, hostGeneration, hostId, refresh])

  return { host, hostId, windowLabel }
}

function queueHostTransition(operation: () => Promise<void>) {
  hostTransition = hostTransition.then(operation, operation).catch(() => {})
}
