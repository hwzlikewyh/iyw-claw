"use client"
import { useCallback, useEffect, useRef } from "react"
import { toErrorMessage } from "@/lib/app-error"
import { getTransport } from "@/lib/transport"
import {
  computerOpenPermissionSettings,
  computerRequestPermission,
  computerRevealHelper,
  computerStatus,
} from "./computer-api"
import {
  computerStoreMark,
  setComputerBackendSince,
  setComputerSharedSince,
} from "./computer-store"
import type { ComputerStatus, OsPermission } from "./types"
import {
  computerHostGeneration,
  useComputerHostState,
  useComputerTransport,
} from "./use-computer-host"
export function useComputerStatus(live: boolean) {
  const [status, setStatus] = useComputerHostState<ComputerStatus | null>(null)
  const [loading, setLoading] = useComputerHostState(false)
  const [error, setError] = useComputerHostState<string | null>(null)
  const [requesting, setRequesting] = useComputerHostState<OsPermission | null>(
    null
  )
  const transport = useComputerTransport()
  const requestingRef = useRef(false)
  const aliveRef = useRef(true)
  const liveRef = useRef(live)
  useEffect(() => {
    liveRef.current = live
  }, [live])
  const seqRef = useRef(0)
  useEffect(() => {
    aliveRef.current = true
    return () => {
      aliveRef.current = false
    }
  }, [])
  const refresh = useCallback(async () => {
    if (transport !== getTransport()) return
    const seq = ++seqRef.current
    const mark = computerStoreMark()
    setLoading(true)
    try {
      const next = await computerStatus()
      if (
        !aliveRef.current ||
        seq !== seqRef.current ||
        transport !== getTransport()
      )
        return
      setStatus(next)
      setComputerSharedSince(next.shared, mark)
      setComputerBackendSince(next.backend, mark)
      setError(null)
    } catch (e) {
      if (aliveRef.current && seq === seqRef.current) {
        setError(toErrorMessage(e))
      }
    } finally {
      if (aliveRef.current && seq === seqRef.current) setLoading(false)
    }
  }, [transport, setStatus, setLoading, setError])
  useEffect(() => {
    if (!live) return
    void refresh()
    const onFocus = () => void refresh()
    const onVisible = () => {
      if (document.visibilityState === "visible") void refresh()
    }
    window.addEventListener("focus", onFocus)
    document.addEventListener("visibilitychange", onVisible)
    return () => {
      window.removeEventListener("focus", onFocus)
      document.removeEventListener("visibilitychange", onVisible)
    }
  }, [live, refresh])
  const request = useCallback(
    async (permission: OsPermission) => {
      if (requestingRef.current || transport !== getTransport()) return
      const hostGeneration = computerHostGeneration()
      requestingRef.current = true
      setRequesting(permission)
      setError(null)
      try {
        const { report, prompted } = await computerRequestPermission(permission)
        if (
          !aliveRef.current ||
          transport !== getTransport() ||
          hostGeneration !== computerHostGeneration()
        )
          return
        const granted =
          permission === "accessibility"
            ? report.accessibility
            : report.screenRecording
        if (report.required && !granted && !prompted) {
          await computerOpenPermissionSettings(permission)
        }
        if (liveRef.current && hostGeneration === computerHostGeneration())
          await refresh()
      } catch (e) {
        setError(toErrorMessage(e))
      } finally {
        requestingRef.current = false
        if (aliveRef.current) setRequesting(null)
      }
    },
    [refresh, transport, setRequesting, setError]
  )
  const openPermissionSettings = useCallback(
    (permission: OsPermission) => {
      if (transport !== getTransport()) return
      computerOpenPermissionSettings(permission).catch((e) =>
        setError(toErrorMessage(e))
      )
    },
    [transport, setError]
  )
  const revealHelper = useCallback(() => {
    if (transport !== getTransport()) return
    computerRevealHelper().catch((e) => setError(toErrorMessage(e)))
  }, [transport, setError])
  return {
    status,
    loading,
    error,
    setError,
    refresh,
    request,
    requesting,
    openPermissionSettings,
    revealHelper,
  }
}
export function hostHoldsPermission(status: ComputerStatus | null): boolean {
  const host = status?.host
  return !!host?.selfResponsible && (host.accessibility || host.screenRecording)
}
