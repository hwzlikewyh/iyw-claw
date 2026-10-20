"use client"

import { useEffect, useState, type RefObject } from "react"

function ownsFullscreen(buttonRef: RefObject<HTMLButtonElement | null>) {
  return Boolean(
    buttonRef.current && document.fullscreenElement?.contains(buttonRef.current)
  )
}

function exitFullscreen() {
  void document.exitFullscreen().catch((error: unknown) => {
    console.warn("[preview] failed to exit native fullscreen", error)
  })
}

export function useNativePreviewFullscreen({
  buttonRef,
  onFullscreen,
  enabled,
}: {
  buttonRef: RefObject<HTMLButtonElement | null>
  onFullscreen?: () => void
  enabled: boolean
}) {
  const [fullscreen, setFullscreen] = useState(false)
  useEffect(() => {
    if (!enabled) return
    const sync = () => setFullscreen(ownsFullscreen(buttonRef))
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !ownsFullscreen(buttonRef)) return
      event.preventDefault()
      event.stopPropagation()
      exitFullscreen()
    }
    document.addEventListener("fullscreenchange", sync)
    window.addEventListener("keydown", onEscape, true)
    return () => {
      document.removeEventListener("fullscreenchange", sync)
      window.removeEventListener("keydown", onEscape, true)
    }
  }, [buttonRef, enabled])
  return {
    fullscreen,
    toggleFullscreen: () => {
      if (ownsFullscreen(buttonRef)) exitFullscreen()
      else onFullscreen?.()
    },
  }
}
