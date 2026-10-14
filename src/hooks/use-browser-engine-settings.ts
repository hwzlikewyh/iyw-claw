import { useEffect, useRef, useState } from "react"
import { browserApi } from "@/lib/browser-api"
import type { BrowserEngineSettings } from "@/lib/browser-types"
import { getShellTransport, isDesktop } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"

export function useBrowserEngineSettings() {
  const [settings, setSettings] = useState<BrowserEngineSettings | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [progress, setProgress] = useState<number | null>(null)
  const [busy, setBusy] = useState<"loading" | "saving" | null>("loading")
  const [installing, setInstalling] = useState(false)
  const mounted = useRef(true)
  const revision = useRef(0)
  useEffect(() => {
    mounted.current = true
    if (!isDesktop()) {
      setBusy(null)
      return
    }
    let disposed = false
    const stops: (() => void)[] = []
    void getShellTransport()
      .subscribe<BrowserEngineSettings>(
        "browser://engine-settings-changed",
        (value) => {
          revision.current += 1
          if (!disposed) {
            setSettings(value)
            setError(null)
          }
        }
      )
      .then(async (stop) => {
        if (disposed) {
          stop()
          return
        }
        stops.push(stop)
        const before = revision.current
        const value = await browserApi.engineSettings()
        if (!disposed && revision.current === before) setSettings(value)
      })
      .catch((cause) => {
        if (!disposed) setError(toErrorMessage(cause))
      })
      .finally(() => {
        if (!disposed) setBusy(null)
      })
    void getShellTransport()
      .subscribe<{ taskId: string; percent?: number }>(
        "app://bootstrap-init",
        (value) => {
          if (
            !disposed &&
            value.taskId === "browser-full-engine" &&
            value.percent !== undefined
          )
            setProgress(value.percent)
        }
      )
      .then((stop) => {
        if (disposed) stop()
        else stops.push(stop)
      })
      .catch((cause) => {
        if (!disposed) setError(toErrorMessage(cause))
      })
    return () => {
      disposed = true
      mounted.current = false
      stops.forEach((stop) => stop())
    }
  }, [])
  const run = async (
    kind: "saving" | "installing",
    operation: () => Promise<BrowserEngineSettings>
  ) => {
    const before = revision.current
    if (kind === "installing") setInstalling(true)
    else setBusy(kind)
    setError(null)
    if (kind === "installing") setProgress(null)
    try {
      const value = await operation()
      if (mounted.current && revision.current === before) setSettings(value)
    } catch (cause) {
      if (mounted.current) setError(toErrorMessage(cause))
    } finally {
      if (mounted.current) {
        if (kind === "installing") setInstalling(false)
        else setBusy(null)
      }
    }
  }
  return {
    settings,
    error,
    busy,
    installing,
    progress,
    save: (enabled: boolean) =>
      run("saving", () => browserApi.setFullEngine(enabled)),
    install: () => run("installing", browserApi.installFullEngine),
  }
}
