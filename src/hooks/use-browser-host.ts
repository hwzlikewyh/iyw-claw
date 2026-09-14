"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { browserApi } from "@/lib/browser-api"
import { normalizeBrowserError } from "@/lib/browser-startup"
import type { BrowserErrorEnvelope } from "@/lib/browser-types"
import { getCurrentWindow, isDesktop } from "@/lib/platform"
import { useBrowser } from "@/contexts/browser-context"

const HEARTBEAT_MS = 2_000
let hostTransition = Promise.resolve()

export function useBrowserHost(kind: "docked" | "detached", enabled: boolean) {
  const { state, acceptState, refresh } = useBrowser()
  const [windowLabel, setWindowLabel] = useState<string | null>(null)
  const [hostId, setHostId] = useState<string | null>(null)
  const [registrationAttempt, setRegistrationAttempt] = useState(0)
  const [error, setError] = useState<BrowserErrorEnvelope | null>(null)
  const retiredHostRef = useRef<string | null>(null)
  const retry = useCallback(
    () => setRegistrationAttempt((value) => value + 1),
    []
  )

  useEffect(() => {
    if (!enabled || !isDesktop()) return
    let disposed = false
    let registeredHostId: string | null = null
    const registration = queueHostTransition(async () => {
      const window = await getCurrentWindow()
      if (disposed) return
      if (!window) throw new Error("The current browser window is unavailable")
      setError(null)
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
    void registration.catch((cause) => {
      if (disposed) return
      const error = normalizeBrowserError(cause)
      setError(error)
      console.warn("[Browser] window registration failed", { code: error.code })
    })
    return () => {
      disposed = true
      setHostId(null)
      void queueHostTransition(async () => {
        if (disposed) {
          const hostId = registeredHostId
          registeredHostId = null
          if (hostId && retiredHostRef.current !== hostId) {
            await browserApi.unregisterHost(hostId)
          }
        }
      }).catch((cause) => {
        console.warn("[Browser] window cleanup failed", {
          code: normalizeBrowserError(cause).code,
        })
      })
    }
  }, [acceptState, enabled, kind, registrationAttempt])

  const host = useMemo(
    () => state?.hosts.find((item) => item.hostId === hostId) ?? null,
    [hostId, state?.hosts]
  )
  const hostGeneration = host?.generation
  const hostMissing = Boolean(hostId && state && !host)
  const runtimeStatus = state?.runtime.status

  // 运行时重试会清空 host；窗口仍挂载时重新注册才能继续创建和绑定页签。
  useEffect(() => {
    if (!enabled || !hostMissing || runtimeStatus !== "running") return
    let cancelled = false
    void refresh().then((latest) => {
      if (
        !cancelled &&
        latest?.runtime.status === "running" &&
        !latest.hosts.some((item) => item.hostId === hostId)
      ) {
        retiredHostRef.current = hostId
        retry()
      }
    })
    return () => {
      cancelled = true
    }
  }, [enabled, hostId, hostMissing, runtimeStatus, retry, refresh])

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

  return { host, hostId, windowLabel, error, retry }
}

function queueHostTransition(operation: () => Promise<void>) {
  const transition = hostTransition.then(operation, operation)
  hostTransition = transition.catch(() => {})
  return transition
}
