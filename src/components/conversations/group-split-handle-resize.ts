"use client"

import { useCallback, useRef, useState, type CSSProperties } from "react"
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
  RefObject,
} from "react"
import type { HandleRect } from "@/lib/tab-group-layout"

const KEYBOARD_RESIZE_STEP = 0.02

export function boundaryFractionForHandle(handle: HandleRect): number {
  if (handle.nodeExtent <= 0) return 0.5
  const boundary = handle.orientation === "horizontal" ? handle.x : handle.y
  return (boundary - handle.nodeStart) / handle.nodeExtent
}

function pointerBoundaryFraction(
  event: ReactPointerEvent<HTMLDivElement>,
  container: HTMLDivElement,
  handle: HandleRect
): number | null {
  const bounds = container.getBoundingClientRect()
  const verticalLine = handle.orientation === "horizontal"
  const axis = verticalLine ? bounds.width : bounds.height
  if (!(axis > 0) || handle.nodeExtent <= 0) return null
  const cursorPercent = verticalLine
    ? ((event.clientX - bounds.left) / axis) * 100
    : ((event.clientY - bounds.top) / axis) * 100
  return (cursorPercent - handle.nodeStart) / handle.nodeExtent
}

export function useSplitPointerResize(
  handle: HandleRect,
  containerRef: RefObject<HTMLDivElement | null>,
  commitResize: (fraction: number) => void
) {
  const [dragging, setDragging] = useState(false)
  const draggingRef = useRef(false)
  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      event.currentTarget.setPointerCapture(event.pointerId)
      draggingRef.current = true
      setDragging(true)
    },
    []
  )
  const onPointerMove = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      const container = containerRef.current
      if (!draggingRef.current || !container) return
      const fraction = pointerBoundaryFraction(event, container, handle)
      if (fraction != null) commitResize(fraction)
    },
    [commitResize, containerRef, handle]
  )
  const endDrag = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return
    draggingRef.current = false
    setDragging(false)
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId)
    }
  }, [])
  const onLostPointerCapture = useCallback(() => {
    draggingRef.current = false
    setDragging(false)
  }, [])
  return {
    dragging,
    onPointerDown,
    onPointerMove,
    endDrag,
    onLostPointerCapture,
  }
}

export function useSplitKeyboardResize(
  handle: HandleRect,
  boundaryFraction: number,
  commitResize: (fraction: number) => void
) {
  return useCallback(
    (event: ReactKeyboardEvent<HTMLDivElement>) => {
      const verticalLine = handle.orientation === "horizontal"
      let next: number | null = null
      if (event.key === "Home") next = 0
      if (event.key === "End") next = 1
      if (verticalLine && event.key === "ArrowLeft") {
        next = boundaryFraction - KEYBOARD_RESIZE_STEP
      }
      if (verticalLine && event.key === "ArrowRight") {
        next = boundaryFraction + KEYBOARD_RESIZE_STEP
      }
      if (!verticalLine && event.key === "ArrowUp") {
        next = boundaryFraction - KEYBOARD_RESIZE_STEP
      }
      if (!verticalLine && event.key === "ArrowDown") {
        next = boundaryFraction + KEYBOARD_RESIZE_STEP
      }
      if (next == null) return
      event.preventDefault()
      commitResize(next)
    },
    [boundaryFraction, commitResize, handle.orientation]
  )
}

export function splitHandleStyle(handle: HandleRect): CSSProperties {
  if (handle.orientation === "horizontal") {
    return {
      left: `calc(${handle.x}% - 4px)`,
      top: `${handle.y}%`,
      width: "9px",
      height: `${handle.length}%`,
    }
  }
  return {
    left: `${handle.x}%`,
    top: `calc(${handle.y}% - 4px)`,
    width: `${handle.length}%`,
    height: "9px",
  }
}
