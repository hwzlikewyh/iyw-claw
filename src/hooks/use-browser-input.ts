"use client"

import { useCallback, useEffect, useRef } from "react"
import type {
  CompositionEvent,
  KeyboardEvent,
  PointerEvent,
  WheelEvent,
} from "react"
import { browserApi } from "@/lib/browser-api"
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

  const send = useCallback(
    (events: BrowserInputEvent[]) => {
      if (!subscription || events.length === 0) return
      void browserApi
        .sendInput(
          subscription.subscriptionId,
          subscription.generations,
          events
        )
        .catch(() => {})
    },
    [subscription]
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
    send(events)
  }, [send])

  const scheduleFlush = useCallback(() => {
    if (rafRef.current !== null) return
    rafRef.current = requestAnimationFrame(() => {
      rafRef.current = null
      flush()
    })
  }, [flush])

  useEffect(
    () => () => {
      flush()
      const pressed = pressedRef.current
      pressedRef.current = null
      if (pressed) {
        send([
          {
            kind: "mouse",
            eventType: "mouseReleased",
            x: pressed.x,
            y: pressed.y,
            button: pressed.button,
            clickCount: 1,
          },
        ])
      }
    },
    [flush, send]
  )

  const point = useCallback(
    (event: { clientX: number; clientY: number }) => {
      const rect = canvasRef.current?.getBoundingClientRect()
      if (!rect) return { x: 0, y: 0 }
      return {
        x: Math.max(0, Math.min(rect.width, event.clientX - rect.left)),
        y: Math.max(0, Math.min(rect.height, event.clientY - rect.top)),
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
      const position = point(event)
      const button = pressed
        ? buttonName(event.button)
        : (pressedRef.current?.button ?? buttonName(event.button))
      if (pressed) {
        pressedRef.current = { pointerId: event.pointerId, button, ...position }
        event.currentTarget.setPointerCapture(event.pointerId)
      } else if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        event.currentTarget.releasePointerCapture(event.pointerId)
      }
      if (!pressed) pressedRef.current = null
      flush()
      send([
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
    [flush, point, send]
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
      send([
        {
          kind: "keyboard",
          eventType: "rawKeyDown",
          key: event.key,
          code: event.code,
          text: event.key.length === 1 ? event.key : undefined,
          windowsVirtualKeyCode: event.keyCode,
          modifiers: modifiers(event),
        },
      ])
    },
    [flush, send]
  )

  const onKeyUp = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.nativeEvent.isComposing || composingRef.current) return
      event.preventDefault()
      send([
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
    [send]
  )

  const onCompositionEnd = useCallback(
    (event: CompositionEvent<HTMLTextAreaElement>) => {
      composingRef.current = false
      if (!event.data) return
      send([
        {
          kind: "keyboard",
          eventType: "char",
          text: event.data,
        },
      ])
      event.currentTarget.value = ""
    },
    [send]
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
  }
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
