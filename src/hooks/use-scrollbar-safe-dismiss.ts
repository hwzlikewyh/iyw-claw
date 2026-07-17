"use client"

import { useCallback, useEffect, useRef } from "react"

type PointerDownOutsideEvent = CustomEvent<{ originalEvent: PointerEvent }>
type FocusOutsideEvent = CustomEvent<{ originalEvent: FocusEvent }>

const INSIDE_POINTER_GRACE_MS = 500

/** Keep a Radix layer open when WebKit bounces focus outside during an internal
 * scrollbar drag, while preserving normal click-away and focus-away dismissal. */
export function useScrollbarSafeDismiss<
  T extends HTMLElement = HTMLDivElement,
>() {
  const contentRef = useRef<T>(null)
  const pointerDownInsideRef = useRef(false)
  const pointerIsDownRef = useRef(false)
  const lastInsidePointerDownAtRef = useRef(0)

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      const content = contentRef.current
      const target = event.target
      const inside =
        !!content && target instanceof Node && content.contains(target)
      pointerDownInsideRef.current = inside
      pointerIsDownRef.current = true
      lastInsidePointerDownAtRef.current = inside ? Date.now() : 0
    }
    const handlePointerUp = () => {
      pointerIsDownRef.current = false
    }
    document.addEventListener("pointerdown", handlePointerDown, true)
    document.addEventListener("pointerup", handlePointerUp, true)
    document.addEventListener("pointercancel", handlePointerUp, true)
    window.addEventListener("blur", handlePointerUp)
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown, true)
      document.removeEventListener("pointerup", handlePointerUp, true)
      document.removeEventListener("pointercancel", handlePointerUp, true)
      window.removeEventListener("blur", handlePointerUp)
    }
  }, [])

  const onPointerDownOutside = useCallback((event: PointerDownOutsideEvent) => {
    const content = contentRef.current
    if (!content) return
    const { clientX, clientY } = event.detail.originalEvent
    const rect = content.getBoundingClientRect()
    if (
      clientX >= rect.left &&
      clientX <= rect.right &&
      clientY >= rect.top &&
      clientY <= rect.bottom
    ) {
      event.preventDefault()
    }
  }, [])

  const onFocusOutside = useCallback((event: FocusOutsideEvent) => {
    const content = contentRef.current
    if (!content) return
    const target = event.detail.originalEvent.target
    const doc = content.ownerDocument
    const droppedToRoot =
      target == null || target === doc.body || target === doc.documentElement
    const movedInsideContent =
      target instanceof Node && content.contains(target)
    const fromInsidePointer =
      (pointerIsDownRef.current && pointerDownInsideRef.current) ||
      Date.now() - lastInsidePointerDownAtRef.current < INSIDE_POINTER_GRACE_MS
    if (droppedToRoot || movedInsideContent || fromInsidePointer) {
      event.preventDefault()
    }
  }, [])

  return { contentRef, onPointerDownOutside, onFocusOutside }
}
