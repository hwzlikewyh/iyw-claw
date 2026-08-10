"use client"

import { useEffect, useState } from "react"
import { usePathname, useSearchParams } from "next/navigation"
import { getCurrentWindow, isLocalDesktop } from "@/lib/platform"
import { rememberRouteForInstaller } from "@/lib/update-restore"

const ROUTE_HEARTBEAT_MS = 5 * 60_000

export function DesktopRouteRestoreTracker() {
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const [isMainWindow, setIsMainWindow] = useState(false)
  const search = searchParams.toString()
  const route = pathname + (search ? `?${search}` : "")

  useEffect(() => {
    if (!isLocalDesktop()) return
    let cancelled = false
    void getCurrentWindow()
      .then((window) => {
        if (!cancelled) setIsMainWindow(window?.label === "main")
      })
      .catch((error) => {
        console.warn("[UpdateRestore] failed to resolve window label", error)
      })
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (!isMainWindow) return
    const remember = () => rememberRouteForInstaller(route)
    remember()
    const timer = window.setInterval(remember, ROUTE_HEARTBEAT_MS)
    window.addEventListener("blur", remember)
    window.addEventListener("focus", remember)
    document.addEventListener("visibilitychange", remember)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener("blur", remember)
      window.removeEventListener("focus", remember)
      document.removeEventListener("visibilitychange", remember)
    }
  }, [isMainWindow, route])

  return null
}
