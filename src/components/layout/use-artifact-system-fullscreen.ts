"use client"

import { useCallback, useEffect, useRef } from "react"
import type { RefObject } from "react"

import { getCurrentWindow, isDesktop } from "@/lib/platform"

export function useArtifactSystemFullscreen({
  enabled,
  targetRef,
  onChange,
}: {
  enabled: boolean
  targetRef: RefObject<HTMLElement | null>
  onChange: (fullscreen: boolean) => void
}): () => Promise<void> {
  const managedRef = useRef(false)
  const syncChange = useCallback(
    (fullscreen: boolean) => {
      if (!fullscreen) managedRef.current = false
      onChange(fullscreen)
    },
    [onChange]
  )
  useDesktopFullscreenSync(enabled, managedRef, syncChange)
  useBrowserFullscreenSync(enabled, managedRef, targetRef, syncChange)

  return useCallback(async () => {
    if (isDesktop()) {
      const window = await getCurrentWindow()
      if (!window) return
      const fullscreen = await window.isFullscreen()
      await window.setFullscreen(!fullscreen)
      managedRef.current = !fullscreen
      syncChange(!fullscreen)
      return
    }
    if (document.fullscreenElement) {
      await document.exitFullscreen()
      managedRef.current = false
    } else {
      await targetRef.current?.requestFullscreen()
      managedRef.current = true
    }
  }, [syncChange, targetRef])
}

function useDesktopFullscreenSync(
  enabled: boolean,
  managedRef: RefObject<boolean>,
  onChange: (fullscreen: boolean) => void
) {
  useEffect(() => {
    if (!isDesktop()) return
    let disposed = false
    let unlisten: (() => void) | undefined

    void getCurrentWindow().then(async (window) => {
      if (!window || disposed) return
      onChange(await window.isFullscreen())
      unlisten = await window.onResized(async () => {
        if (!disposed) onChange(await window.isFullscreen())
      })
    })

    return () => {
      disposed = true
      unlisten?.()
      if (!enabled || !managedRef.current) return
      managedRef.current = false
      onChange(false)
      void getCurrentWindow().then((window) => window?.setFullscreen(false))
    }
  }, [enabled, managedRef, onChange])
}

function useBrowserFullscreenSync(
  enabled: boolean,
  managedRef: RefObject<boolean>,
  targetRef: RefObject<HTMLElement | null>,
  onChange: (fullscreen: boolean) => void
) {
  useEffect(() => {
    if (isDesktop()) return
    const fullscreenTarget = targetRef.current
    const update = () => {
      onChange(document.fullscreenElement === targetRef.current)
    }
    document.addEventListener("fullscreenchange", update)
    update()
    return () => {
      document.removeEventListener("fullscreenchange", update)
      if (
        enabled &&
        managedRef.current &&
        document.fullscreenElement === fullscreenTarget
      ) {
        managedRef.current = false
        onChange(false)
        void document.exitFullscreen()
      }
    }
  }, [enabled, managedRef, onChange, targetRef])
}
