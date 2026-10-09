"use client"
import { useCallback, useEffect, useRef } from "react"
import { toErrorMessage } from "@/lib/app-error"
import { subscribe } from "@/lib/platform"
import { getTransport } from "@/lib/transport"
import {
  computerDriverInfo,
  computerDriverInstall,
  computerDriverUninstall,
  useComputerAvailable,
} from "./computer-api"
import { COMPUTER_DRIVER_EVENT, type DriverInfo } from "./types"
import { useComputerHostState, useComputerTransport } from "./use-computer-host"
export function useComputerDriver() {
  const [info, setInfo] = useComputerHostState<DriverInfo | null>(null)
  const [error, setError] = useComputerHostState<string | null>(null)
  const transport = useComputerTransport()
  const heardRef = useRef(0)
  const available = useComputerAvailable()
  useEffect(() => {
    if (!available) return
    let disposed = false
    let unsubscribe: (() => void) | undefined
    const asked = heardRef.current
    const ask = () => {
      if (disposed) return
      computerDriverInfo()
        .then((read) => {
          if (
            !disposed &&
            heardRef.current === asked &&
            transport === getTransport()
          )
            setInfo(read)
        })
        .catch((e) => {
          if (!disposed) setError(toErrorMessage(e))
        })
    }
    subscribe<DriverInfo>(COMPUTER_DRIVER_EVENT, (next) => {
      if (disposed || transport !== getTransport()) return
      heardRef.current += 1
      setInfo(next)
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
  }, [available, transport, setInfo, setError])
  const run = useCallback(
    async (action: () => Promise<DriverInfo>) => {
      if (transport !== getTransport()) return false
      setError(null)
      const since = heardRef.current
      try {
        const done = await action()
        if (transport !== getTransport()) return false
        if (heardRef.current === since) setInfo(done)
        return true
      } catch (e) {
        setError(toErrorMessage(e))
        return false
      }
    },
    [transport, setInfo, setError]
  )
  const install = useCallback(() => run(computerDriverInstall), [run])
  const uninstall = useCallback(() => run(computerDriverUninstall), [run])
  return { info, error, install, uninstall }
}
