import { useRef, type PointerEvent } from "react"
import { isDesktop } from "@/lib/transport"

const DRAG_DISTANCE = 32

export function usePluginTabDrag(detach: () => void) {
  const start = useRef<{
    x: number
    y: number
    bottom: number
    top: number
  } | null>(null)
  const dragged = useRef(false)
  return {
    onPointerDown: (event: PointerEvent<HTMLButtonElement>) => {
      if (!isDesktop() || event.button !== 0 || event.pointerType === "touch")
        return
      const bounds = event.currentTarget.closest("nav")?.getBoundingClientRect()
      if (!bounds) return
      start.current = {
        x: event.clientX,
        y: event.clientY,
        bottom: bounds.bottom,
        top: bounds.top,
      }
      dragged.current = false
      event.currentTarget.setPointerCapture(event.pointerId)
    },
    onPointerUp: (event: PointerEvent<HTMLButtonElement>) => {
      const origin = start.current
      start.current = null
      if (!origin) return
      const moved =
        Math.hypot(event.clientX - origin.x, event.clientY - origin.y) >=
        DRAG_DISTANCE
      const outside =
        event.clientY > origin.bottom + DRAG_DISTANCE ||
        event.clientY < origin.top - DRAG_DISTANCE ||
        event.clientX < 0 ||
        event.clientX >= window.innerWidth
      if (moved && outside) {
        dragged.current = true
        detach()
      }
    },
    onPointerCancel: () => {
      start.current = null
    },
    onLostPointerCapture: () => {
      start.current = null
    },
    consumeDrag: () => {
      const moved = dragged.current
      dragged.current = false
      return moved
    },
  }
}
