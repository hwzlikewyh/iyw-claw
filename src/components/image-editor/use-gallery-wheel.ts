"use client"

import { useCallback, useRef, type RefObject } from "react"
import type { ImagePreviewNavigation } from "./image-editor-model"

const WHEEL_THRESHOLD = 40
const WHEEL_COOLDOWN_MS = 250
const LINE_PIXELS = 16
const PAGE_PIXELS = 100

export interface GalleryWheelGesture {
  delta: number
  lastEvent: number
  lastSwitch: number
}

export function useGalleryWheel(
  navigation: ImagePreviewNavigation | undefined,
  options: {
    enabled?: boolean
    gesture?: RefObject<GalleryWheelGesture>
  } = {}
) {
  const ownGesture = useRef<GalleryWheelGesture>({
    delta: 0,
    lastEvent: 0,
    lastSwitch: -Infinity,
  })
  const gesture = options.gesture ?? ownGesture
  const enabled = options.enabled ?? true
  return useCallback(
    (element: HTMLDivElement | null) => {
      if (!element || !enabled || !navigation || navigation.total <= 1) return
      const handleWheel = (event: WheelEvent) => {
        if (event.ctrlKey || event.metaKey || event.altKey) return
        if (event.deltaX === 0 && event.deltaY === 0) return
        event.preventDefault()
        event.stopPropagation()
        const direction = wheelDirection(event, gesture.current)
        if (!direction) return
        const index = Math.max(
          0,
          Math.min(navigation.total - 1, navigation.index + direction)
        )
        if (index !== navigation.index) navigation.onIndexChange(index)
      }
      element.addEventListener("wheel", handleWheel, {
        passive: false,
        capture: true,
      })
      return () => element.removeEventListener("wheel", handleWheel, true)
    },
    [enabled, gesture, navigation]
  )
}

function wheelDirection(event: WheelEvent, state: GalleryWheelGesture): number {
  const delta =
    Math.abs(event.deltaX) > Math.abs(event.deltaY)
      ? event.deltaX
      : event.deltaY
  const unit =
    event.deltaMode === 1
      ? LINE_PIXELS
      : event.deltaMode === 2
        ? PAGE_PIXELS
        : 1
  const now = event.timeStamp
  if (now - state.lastEvent > WHEEL_COOLDOWN_MS || state.delta * delta < 0) {
    state.delta = 0
  }
  state.lastEvent = now
  if (now - state.lastSwitch < WHEEL_COOLDOWN_MS) return 0
  state.delta += delta * unit
  if (Math.abs(state.delta) < WHEEL_THRESHOLD) return 0
  state.delta = 0
  state.lastSwitch = now
  return Math.sign(delta)
}
