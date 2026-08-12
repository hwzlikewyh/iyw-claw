"use client"

import { useCallback, useEffect, useRef, useState } from "react"

export const ZOOM_MIN = 0.1
export const ZOOM_MAX = 10
const ZOOM_STEP = 0.25

export function useImageZoom() {
  const [zoom, setZoom] = useState(1)
  const zoomIn = useCallback(
    () => setZoom((value) => Math.min(ZOOM_MAX, value + ZOOM_STEP)),
    []
  )
  const zoomOut = useCallback(
    () => setZoom((value) => Math.max(ZOOM_MIN, value - ZOOM_STEP)),
    []
  )
  const resetZoom = useCallback(() => setZoom(1), [])
  return { zoom, setZoom, zoomIn, zoomOut, resetZoom }
}

export function useImageViewport(
  setZoom: React.Dispatch<React.SetStateAction<number>>
) {
  const [size, setSize] = useState({ width: 0, height: 0 })
  const elementRef = useRef<HTMLDivElement>(null)
  const observerRef = useRef<ResizeObserver | null>(null)
  const wheelRef = useRef((event: WheelEvent) => {
    if (!event.ctrlKey && !event.metaKey) return
    event.preventDefault()
    const delta = event.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP
    setZoom((value) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, value + delta)))
  })
  const viewportRef = useCallback((element: HTMLDivElement | null) => {
    disconnectViewport(
      elementRef.current,
      observerRef.current,
      wheelRef.current
    )
    elementRef.current = element
    observerRef.current = element ? observeSize(element, setSize) : null
    element?.addEventListener("wheel", wheelRef.current, { passive: false })
  }, [])
  return { size, elementRef, viewportRef }
}

function disconnectViewport(
  element: HTMLDivElement | null,
  observer: ResizeObserver | null,
  wheelHandler: (event: WheelEvent) => void
) {
  element?.removeEventListener("wheel", wheelHandler)
  observer?.disconnect()
}

function observeSize(
  element: HTMLDivElement,
  update: React.Dispatch<
    React.SetStateAction<{ width: number; height: number }>
  >
) {
  const observer = new ResizeObserver(([entry]) => {
    if (entry) {
      update({
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      })
    }
  })
  observer.observe(element)
  return observer
}

interface DragState {
  startX: number
  startY: number
  scrollX: number
  scrollY: number
}

export function useRightButtonPan(
  viewportRef: React.RefObject<HTMLDivElement | null>
) {
  const dragRef = useRef<DragState | null>(null)
  const onMouseDown = useCallback(
    (event: React.MouseEvent) => startPan(event, viewportRef.current, dragRef),
    [viewportRef]
  )
  useEffect(() => bindPanEvents(viewportRef, dragRef), [viewportRef])
  const onContextMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault()
  }, [])
  return { onMouseDown, onContextMenu }
}

function startPan(
  event: React.MouseEvent,
  element: HTMLDivElement | null,
  dragRef: React.MutableRefObject<DragState | null>
) {
  if (event.button !== 2 || !element) return
  event.preventDefault()
  dragRef.current = {
    startX: event.clientX,
    startY: event.clientY,
    scrollX: element.scrollLeft,
    scrollY: element.scrollTop,
  }
  element.style.cursor = "grabbing"
}

function bindPanEvents(
  viewportRef: React.RefObject<HTMLDivElement | null>,
  dragRef: React.MutableRefObject<DragState | null>
) {
  const move = (event: MouseEvent) => {
    const drag = dragRef.current
    const element = viewportRef.current
    if (!drag || !element) return
    element.scrollLeft = drag.scrollX - (event.clientX - drag.startX)
    element.scrollTop = drag.scrollY - (event.clientY - drag.startY)
  }
  const stop = () => {
    dragRef.current = null
    if (viewportRef.current) viewportRef.current.style.cursor = ""
  }
  window.addEventListener("mousemove", move)
  window.addEventListener("mouseup", stop)
  return () => {
    window.removeEventListener("mousemove", move)
    window.removeEventListener("mouseup", stop)
  }
}
