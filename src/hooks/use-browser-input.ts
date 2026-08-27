"use client"

import { useCallback, useEffect, useRef } from "react"
import type {
  CompositionEvent,
  KeyboardEvent,
  PointerEvent,
  WheelEvent,
} from "react"
import { useBrowserInputTransport } from "./use-browser-input-transport"
import type {
  BrowserFrameSubscriptionSnapshot,
  BrowserInputEvent,
} from "@/lib/browser-types"

export function useBrowserInput(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  subscription: BrowserFrameSubscriptionSnapshot | null
) {
  const moveRef = useRef<BrowserInputEvent | null>(null)
  const wheelRef = useRef<BrowserInputEvent | null>(null)
  const rafRef = useRef<number | null>(null)
  const composingRef = useRef(false)
  const pressedRef = useRef<{
    pointerId: number
    button: "left" | "middle" | "right"
    x: number
    y: number
  } | null>(null)
  const { enqueue, releasePressed, inputError } = useBrowserInputTransport(
    canvasRef,
    subscription,
    pressedRef
  )

  const flush = useCallback(() => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current)
      rafRef.current = null
    }
    const events = [moveRef.current, wheelRef.current].filter(
      (event): event is BrowserInputEvent => event !== null
    )
    moveRef.current = null
    wheelRef.current = null
    enqueue(events)
  }, [enqueue])

  useEffect(() => {
    return () => {
      flush()
      releasePressed()
    }
  }, [flush, releasePressed])

  const scheduleFlush = useCallback(() => {
    if (rafRef.current !== null) return
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null
      flush()
    })
  }, [flush])

  const point = useCallback(
    (event: { clientX: number; clientY: number }) => {
      const rect = canvasRef.current?.getBoundingClientRect()
      if (!rect) return { x: 0, y: 0 }
      const logicalWidth = readViewportDimension(
        canvasRef.current?.dataset.browserViewportWidth,
        rect.width
      )
      const logicalHeight = readViewportDimension(
        canvasRef.current?.dataset.browserViewportHeight,
        rect.height
      )
      return {
        x: Math.max(
          0,
          Math.min(
            logicalWidth,
            ((event.clientX - rect.left) / Math.max(1, rect.width)) *
              logicalWidth
          )
        ),
        y: Math.max(
          0,
          Math.min(
            logicalHeight,
            ((event.clientY - rect.top) / Math.max(1, rect.height)) *
              logicalHeight
          )
        ),
      }
    },
    [canvasRef]
  )

  const onPointerMove = useCallback(
    (event: PointerEvent<HTMLCanvasElement>) => {
      const position = point(event)
      if (pressedRef.current?.pointerId === event.pointerId) {
        pressedRef.current.x = position.x
        pressedRef.current.y = position.y
      }
      moveRef.current = {
        kind: "mouse",
        eventType: "mouseMoved",
        ...position,
        button: "none",
        modifiers: modifiers(event),
      }
      scheduleFlush()
    },
    [point, scheduleFlush]
  )

  const pointerButton = useCallback(
    (event: PointerEvent<HTMLCanvasElement>, pressed: boolean) => {
      event.preventDefault()
      if (pressed && pressedRef.current) releasePressed()
      const position = point(event)
      const active = pressedRef.current
      if (!pressed && active && active.pointerId !== event.pointerId) return
      const button = pressed
        ? buttonName(event.button)
        : (active?.button ?? buttonName(event.button))
      if (pressed) {
        pressedRef.current = { pointerId: event.pointerId, button, ...position }
        event.currentTarget.setPointerCapture(event.pointerId)
      } else {
        pressedRef.current = null
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId)
        }
      }
      flush()
      enqueue([
        {
          kind: "mouse",
          eventType: pressed ? "mousePressed" : "mouseReleased",
          ...position,
          button,
          clickCount: event.detail > 0 ? Math.min(3, event.detail) : 1,
          modifiers: modifiers(event),
        },
      ])
    },
    [enqueue, flush, point, releasePressed]
  )

  const onWheel = useCallback(
    (event: WheelEvent<HTMLCanvasElement>) => {
      event.preventDefault()
      const current = wheelRef.current
      wheelRef.current = {
        kind: "mouse",
        eventType: "mouseWheel",
        ...point(event),
        button: "none",
        deltaX:
          (current?.kind === "mouse" ? (current.deltaX ?? 0) : 0) +
          event.deltaX,
        deltaY:
          (current?.kind === "mouse" ? (current.deltaY ?? 0) : 0) +
          event.deltaY,
        modifiers: modifiers(event),
      }
      scheduleFlush()
    },
    [point, scheduleFlush]
  )

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.nativeEvent.isComposing || composingRef.current) return
      event.preventDefault()
      flush()
      const printable = event.key.length === 1
      const emitsChar =
        printable && !event.ctrlKey && !event.altKey && !event.metaKey
      const events: BrowserInputEvent[] = [
        {
          kind: "keyboard",
          eventType: emitsChar ? "keyDown" : "rawKeyDown",
          key: event.key,
          code: event.code,
          windowsVirtualKeyCode: event.keyCode,
          modifiers: modifiers(event),
        },
      ]
      if (emitsChar)
        events.push({ kind: "keyboard", eventType: "char", text: event.key })
      enqueue(events)
    },
    [enqueue, flush]
  )

  const onKeyUp = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.nativeEvent.isComposing || composingRef.current) return
      event.preventDefault()
      enqueue([
        {
          kind: "keyboard",
          eventType: "keyUp",
          key: event.key,
          code: event.code,
          windowsVirtualKeyCode: event.keyCode,
          modifiers: modifiers(event),
        },
      ])
    },
    [enqueue]
  )

  const onCompositionEnd = useCallback(
    (event: CompositionEvent<HTMLTextAreaElement>) => {
      composingRef.current = false
      if (!event.data) return
      enqueue([{ kind: "keyboard", eventType: "char", text: event.data }])
      event.currentTarget.value = ""
    },
    [enqueue]
  )

  return {
    canvasProps: {
      onPointerMove,
      onPointerDown: (event: PointerEvent<HTMLCanvasElement>) =>
        pointerButton(event, true),
      onPointerUp: (event: PointerEvent<HTMLCanvasElement>) =>
        pointerButton(event, false),
      onPointerCancel: (event: PointerEvent<HTMLCanvasElement>) =>
        pointerButton(event, false),
      onWheel,
      onContextMenu: (event: React.MouseEvent) => event.preventDefault(),
    },
    textProps: {
      onKeyDown,
      onKeyUp,
      onCompositionStart: () => {
        composingRef.current = true
      },
      onCompositionEnd,
    },
    inputError,
  }
}

function readViewportDimension(
  value: string | undefined,
  fallback: number
): number {
  const parsed = value ? Number(value) : fallback
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function modifiers(event: {
  altKey: boolean
  ctrlKey: boolean
  metaKey: boolean
  shiftKey: boolean
}) {
  return (
    (event.altKey ? 1 : 0) |
    (event.ctrlKey ? 2 : 0) |
    (event.metaKey ? 4 : 0) |
    (event.shiftKey ? 8 : 0)
  )
}

function buttonName(button: number): "left" | "middle" | "right" {
  if (button === 1) return "middle"
  if (button === 2) return "right"
  return "left"
}
