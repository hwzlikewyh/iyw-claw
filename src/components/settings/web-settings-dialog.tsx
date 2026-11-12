"use client"

import { lazy, Suspense, useEffect, useState } from "react"
import { isDesktop } from "@/lib/transport"

const SettingsDialog = lazy(() =>
  import("./settings-dialog").then((module) => ({
    default: module.SettingsDialog,
  }))
)

export function WebSettingsDialog() {
  const [enabled, setEnabled] = useState(false)

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => setEnabled(!isDesktop()))
    return () => window.cancelAnimationFrame(frame)
  }, [])

  if (!enabled) return null

  return (
    <Suspense fallback={null}>
      <SettingsDialog />
    </Suspense>
  )
}
