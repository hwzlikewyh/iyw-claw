"use client"

import { useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { isDesktop } from "@/lib/platform"

export function DevtoolsShortcut() {
  useEffect(() => {
    if (!isDesktop()) return

    const openDevtools = (event: KeyboardEvent) => {
      if (
        event.code !== "KeyT" ||
        !event.ctrlKey ||
        !event.altKey ||
        event.metaKey ||
        event.shiftKey ||
        event.repeat ||
        event.getModifierState("AltGraph")
      ) {
        return
      }

      event.preventDefault()
      void invoke("open_devtools").catch((error) => {
        console.error("Failed to open developer tools", error)
      })
    }

    window.addEventListener("keydown", openDevtools, true)
    return () => window.removeEventListener("keydown", openDevtools, true)
  }, [])

  return null
}
