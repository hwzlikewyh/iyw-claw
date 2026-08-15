"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { browserApi } from "@/lib/browser-api"
import { getCurrentWindow, isDesktop } from "@/lib/platform"
import { useBrowser } from "@/contexts/browser-context"

const HEARTBEAT_MS = 2_000

export function useBrowserHost(kind: "docked" | "detached", enabled: boolean) {
  const { state, acceptState, refresh } = useBrowser()
  const [windowLabel, setWindowLabel] = useState<string | null>(null)
  const [hostId, setHostId] = useState<string | null>(null)
  const reclaimingRef = useRef(false)

  useEffect(() => {
    if (!enabled || !isDesktop()) return
    let disposed = false
    let registeredHostId: string | null = null
    void getCurrentWindow()
      .then((window) => {
        if (!window || disposed) return null
        setWindowLabel(window.label)
        return browserApi.registerHost(window.label, kind)
      })
      .then((registration) => {
        if (!registration) return
        if (disposed) {
          void browserApi.unregisterHost(registration.hostId).catch(() => {})
          return
        }
        registeredHostId = registration.hostId
        setHostId(registration.hostId)
        acceptState(registration.state)
      })
      .catch(() => {})
    return () => {
      disposed = true
      if (registeredHostId) {
        void browserApi.unregisterHost(registeredHostId).catch(() => {})
      }
      setHostId(null)
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

  useEffect(() => {
    if (kind !== "docked" || !host || reclaimingRef.current) return
    if (state?.runtime.status !== "running") return
    const hasTargetClaim = state.viewClaims.some(
      (claim) => claim.targetHostId === host.hostId
    )
    const unclaimed = state.tabs.find(
      (tab) => tab.viewStatus === "unclaimed" && tab.status === "live"
    )
    if (!unclaimed || hasTargetClaim) return
    reclaimingRef.current = true
    void browserApi
      .beginClaim(
        unclaimed.browserTabId,
        undefined,
        host.hostId,
        host.tabOrder.length
      )
      .then(() => refresh())
      .finally(() => {
        reclaimingRef.current = false
      })
  }, [host, kind, refresh, state])

  return { host, hostId, windowLabel }
}
