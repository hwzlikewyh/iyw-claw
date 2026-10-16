"use client"

import { useEffect } from "react"
import { getCurrentWindow, isDesktop } from "@/lib/platform"
import { getShellTransport } from "@/lib/transport"
import {
  BROWSER_SESSION_REQUEST,
  handleBrowserSessionRequest,
  type BrowserSessionRequest,
} from "@/lib/browser-session-handoff"

export function useBrowserSessionBridge() {
  useEffect(() => {
    if (!isDesktop()) return
    let disposed = false
    let stop: (() => void) | undefined
    void getCurrentWindow()
      .then((current) => {
        if (disposed || current?.label !== "main") return
        return getShellTransport().subscribe<BrowserSessionRequest>(
          BROWSER_SESSION_REQUEST,
          (request) => {
            if (disposed) return
            void handleBrowserSessionRequest(request).catch((error) => {
              console.warn("[Browser] conversation reply failed", { error })
            })
          }
        )
      })
      .then((unsubscribe) => {
        if (!unsubscribe) return
        if (disposed) unsubscribe()
        else stop = unsubscribe
      })
      .catch((error) => {
        console.warn("[Browser] conversation bridge subscription failed", {
          error,
        })
      })
    return () => {
      disposed = true
      stop?.()
    }
  }, [])
}
