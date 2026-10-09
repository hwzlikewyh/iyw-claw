"use client"
import { useEffect } from "react"
import { subscribe } from "@/lib/platform"
import { getTransport } from "@/lib/transport"
import { computerStopKeyStatus, useComputerAvailable } from "./computer-api"
import { COMPUTER_STOP_KEY_EVENT, type StopKeyStatus } from "./types"
import { useComputerHostState, useComputerTransport } from "./use-computer-host"
export function useComputerStopKey(): StopKeyStatus | null {
  const [status, setStatus] = useComputerHostState<StopKeyStatus | null>(null)
  const transport = useComputerTransport()
  const available = useComputerAvailable()
  useEffect(() => {
    if (!available) return
    let disposed = false
    let broadcasts = 0
    let unsubscribe: (() => void) | undefined
    const ask = () => {
      if (disposed) return
      computerStopKeyStatus()
        .then((s) => {
          if (!disposed && broadcasts === 0) setStatus(s)
        })
        .catch(() => {})
    }
    void subscribe<StopKeyStatus>(COMPUTER_STOP_KEY_EVENT, (s) => {
      if (disposed || transport !== getTransport()) return
      broadcasts += 1
      setStatus(s)
    })
      .then((fn) => {
        if (disposed) fn()
        else unsubscribe = fn
      })
      .catch(() => {})
      .finally(ask)
    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [available, transport, setStatus])
  return status
}
