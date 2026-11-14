import { useCallback, useEffect, useRef, useState } from "react"

const INITIALIZATION_TIMEOUT_MS = 30_000

export function usePluginAppInitialization(
  instanceId: string,
  enabled: boolean
) {
  const [failure, setFailure] = useState<string | null>(null)
  const [readyInstance, setReadyInstance] = useState<string | null>(null)
  const initialized = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  useEffect(() => {
    initialized.current = false
    if (enabled)
      timer.current = setTimeout(() => {
        if (!initialized.current) setFailure(instanceId)
      }, INITIALIZATION_TIMEOUT_MS)
    return () => clearTimeout(timer.current)
  }, [instanceId, enabled])
  const ready = useCallback(() => {
    initialized.current = true
    setReadyInstance(instanceId)
    clearTimeout(timer.current)
  }, [instanceId])
  const fail = useCallback(() => {
    if (!initialized.current) setFailure(instanceId)
  }, [instanceId])
  return {
    failed: failure === instanceId,
    initialized: readyInstance === instanceId,
    ready,
    fail,
  }
}
