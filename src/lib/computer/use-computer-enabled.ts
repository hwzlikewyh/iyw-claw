"use client"
import { useCallback, useEffect, useRef } from "react"
import { subscribe } from "@/lib/platform"
import { getTransport } from "@/lib/transport"
import { getComputerToolsSettings, useComputerAvailable } from "./computer-api"
import {
  COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT,
  type ComputerToolsSettings,
} from "./types"
import { useComputerHostState, useComputerTransport } from "./use-computer-host"
export function useComputerEnabled({ desktopOnly }: { desktopOnly: boolean }) {
  const [enabled, setEnabled] = useComputerHostState<boolean | null>(null)
  const transport = useComputerTransport()
  const heardRef = useRef(0)
  const available = useComputerAvailable()
  useEffect(() => {
    if (desktopOnly && !available) return
    let disposed = false
    let unsubscribe: (() => void) | undefined
    const asked = heardRef.current
    const ask = () => {
      if (disposed) return
      getComputerToolsSettings()
        .then((s) => {
          if (
            !disposed &&
            heardRef.current === asked &&
            transport === getTransport()
          )
            setEnabled(s.enabled)
        })
        .catch(() => {})
    }
    subscribe<ComputerToolsSettings>(
      COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT,
      (s) => {
        if (disposed || transport !== getTransport()) return
        heardRef.current += 1
        setEnabled(s.enabled)
      }
    )
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
  }, [desktopOnly, available, transport, setEnabled])
  const mark = useCallback(() => heardRef.current, [])
  const applySince = useCallback(
    (settings: ComputerToolsSettings, since: number) => {
      if (heardRef.current === since) setEnabled(settings.enabled)
    },
    [setEnabled]
  )
  return { enabled, mark, applySince }
}
