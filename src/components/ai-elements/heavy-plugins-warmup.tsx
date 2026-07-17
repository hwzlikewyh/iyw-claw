"use client"

import { useEffect } from "react"

import { prefetchHeavyPlugins } from "./streamdown-plugins"

/** Warm the code highlighter after first paint, before streaming needs it. */
export function HeavyPluginsWarmup() {
  useEffect(() => {
    let warmed = false
    function warm() {
      if (warmed) return
      warmed = true
      window.removeEventListener("pointerdown", warm, true)
      window.removeEventListener("keydown", warm, true)
      prefetchHeavyPlugins(["code"])
    }

    window.addEventListener("pointerdown", warm, true)
    window.addEventListener("keydown", warm, true)

    let cancelIdle: () => void
    if (typeof window.requestIdleCallback === "function") {
      const handle = window.requestIdleCallback(warm, { timeout: 3000 })
      cancelIdle = () => {
        if (typeof window.cancelIdleCallback === "function") {
          window.cancelIdleCallback(handle)
        }
      }
    } else {
      const timer = setTimeout(warm, 1500)
      cancelIdle = () => clearTimeout(timer)
    }

    return () => {
      window.removeEventListener("pointerdown", warm, true)
      window.removeEventListener("keydown", warm, true)
      cancelIdle()
    }
  }, [])

  return null
}
