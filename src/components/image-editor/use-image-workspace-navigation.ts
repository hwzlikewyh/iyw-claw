"use client"

import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type PointerEvent,
  type RefObject,
  type SetStateAction,
} from "react"
import { clampImageZoom } from "./image-editor-model"

interface NavigationOptions {
  ready: boolean
  busy: boolean
  setZoom: Dispatch<SetStateAction<number>>
}

interface PanState {
  pointerId: number
  x: number
  y: number
  scrollLeft: number
  scrollTop: number
}

const WHEEL_ZOOM_SENSITIVITY = 0.0015
const WHEEL_LINE_PIXELS = 16
const WHEEL_PAGE_PIXELS = 100
const DOM_DELTA_LINE = 1
const DOM_DELTA_PAGE = 2

function wheelPixels(event: WheelEvent): number {
  const unit =
    event.deltaMode === DOM_DELTA_LINE
      ? WHEEL_LINE_PIXELS
      : event.deltaMode === DOM_DELTA_PAGE
        ? WHEEL_PAGE_PIXELS
        : 1
  return event.deltaY * unit
}

export function useImageWorkspaceNavigation(
  elementRef: RefObject<HTMLDivElement | null>,
  options: NavigationOptions
) {
  const { ready, busy, setZoom } = options
  const panRef = useRef<PanState | null>(null)
  const [panning, setPanning] = useState(false)
  useEffect(() => {
    const element = elementRef.current
    if (!element) return
    const handleWheel = (event: WheelEvent) => {
      if (!ready || busy) return
      event.preventDefault()
      const factor = Math.exp(-wheelPixels(event) * WHEEL_ZOOM_SENSITIVITY)
      setZoom((zoom) => clampImageZoom(zoom * factor))
    }
    element.addEventListener("wheel", handleWheel, { passive: false })
    return () => element.removeEventListener("wheel", handleWheel)
  }, [busy, elementRef, ready, setZoom])
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 1 || !ready || busy) return
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    panRef.current = {
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      scrollLeft: event.currentTarget.scrollLeft,
      scrollTop: event.currentTarget.scrollTop,
    }
    setPanning(true)
  }
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const pan = panRef.current
    if (!pan || pan.pointerId !== event.pointerId) return
    event.currentTarget.scrollLeft = pan.scrollLeft - (event.clientX - pan.x)
    event.currentTarget.scrollTop = pan.scrollTop - (event.clientY - pan.y)
  }
  const endPan = (event: PointerEvent<HTMLDivElement>) => {
    if (panRef.current?.pointerId !== event.pointerId) return
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
    panRef.current = null
    setPanning(false)
  }
  return { panning, onPointerDown, onPointerMove, endPan }
}
