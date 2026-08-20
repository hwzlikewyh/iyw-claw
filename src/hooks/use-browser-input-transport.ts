"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { browserApi } from "@/lib/browser-api"
import type {
  BrowserFrameSubscriptionSnapshot,
  BrowserGenerations,
  BrowserInputEvent,
} from "@/lib/browser-types"

export type BrowserPressedPointer = {
  pointerId: number
  button: "left" | "middle" | "right"
  x: number
  y: number
}

type QueuedInputBatch = {
  subscriptionId: string
  generations: BrowserGenerations
  events: BrowserInputEvent[]
}

export function useBrowserInputTransport(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  subscription: BrowserFrameSubscriptionSnapshot | null,
  pressedRef: React.MutableRefObject<BrowserPressedPointer | null>
) {
  const subscriptionRef = useRef(subscription)
  const queueRef = useRef<QueuedInputBatch[]>([])
  const drainingRef = useRef(false)
  const disposedRef = useRef(false)
  const [inputError, setInputError] = useState(false)

  const drain = useCallback(async () => {
    if (drainingRef.current) return
    drainingRef.current = true
    try {
      while (!disposedRef.current && queueRef.current.length > 0) {
        const batch = queueRef.current.shift()
        if (!batch || !sameSubscription(batch, subscriptionRef.current))
          continue
        try {
          await browserApi.sendInput(
            batch.subscriptionId,
            batch.generations,
            batch.events
          )
          if (!disposedRef.current) setInputError(false)
        } catch {
          queueRef.current = []
          const pressed = pressedRef.current
          pressedRef.current = null
          if (pressed) clearPointerCapture(canvasRef, pressed.pointerId)
          if (!disposedRef.current) setInputError(true)
          break
        }
      }
    } finally {
      drainingRef.current = false
    }
  }, [canvasRef, pressedRef])

  const enqueue = useCallback(
    (events: BrowserInputEvent[]) => {
      const current = subscriptionRef.current
      if (!current || events.length === 0 || disposedRef.current) return
      const batch = {
        subscriptionId: current.subscriptionId,
        generations: current.generations,
        events,
      }
      queueRef.current = isMouseMoveBatch(events)
        ? coalesceMouseMoves(queueRef.current, batch)
        : queueRef.current
            .filter(
              (item) =>
                !sameSubscription(item, current) ||
                !isMouseMoveBatch(item.events)
            )
            .concat(batch)
      void drain()
    },
    [drain]
  )

  const releasePressed = useCallback(() => {
    const pressed = pressedRef.current
    if (!pressed) return
    pressedRef.current = null
    clearPointerCapture(canvasRef, pressed.pointerId)
    enqueue([releaseEvent(pressed)])
  }, [canvasRef, enqueue, pressedRef])

  useEffect(() => {
    const previous = subscriptionRef.current
    if (
      previous &&
      (!subscription || previous.subscriptionId !== subscription.subscriptionId)
    ) {
      const pressed = pressedRef.current
      if (pressed) {
        pressedRef.current = null
        clearPointerCapture(canvasRef, pressed.pointerId)
        queueRef.current = []
        void browserApi
          .sendInput(previous.subscriptionId, previous.generations, [
            releaseEvent(pressed),
          ])
          .catch(() => {})
      }
    }
    subscriptionRef.current = subscription
    queueRef.current = subscription
      ? queueRef.current.filter((item) => sameSubscription(item, subscription))
      : []
    if (subscription) setInputError(false)
  }, [canvasRef, pressedRef, releasePressed, subscription])

  useEffect(() => {
    disposedRef.current = false
    const onWindowBlur = () => releasePressed()
    window.addEventListener("blur", onWindowBlur)
    return () => {
      releasePressed()
      disposedRef.current = true
      queueRef.current = []
      window.removeEventListener("blur", onWindowBlur)
    }
  }, [releasePressed])

  return { enqueue, releasePressed, inputError }
}

function sameSubscription(
  batch: Pick<QueuedInputBatch, "subscriptionId">,
  current: { subscriptionId: string } | null
) {
  return current?.subscriptionId === batch.subscriptionId
}

function isMouseMoveBatch(events: BrowserInputEvent[]) {
  return (
    events.length > 0 &&
    events.every(
      (event) => event.kind === "mouse" && event.eventType === "mouseMoved"
    )
  )
}

function coalesceMouseMoves(
  queue: QueuedInputBatch[],
  batch: QueuedInputBatch
) {
  const tail = queue[queue.length - 1]
  if (tail && sameSubscription(tail, batch) && isMouseMoveBatch(tail.events)) {
    tail.events = batch.events
    return queue
  }
  return queue
    .filter(
      (item) => !sameSubscription(item, batch) || !isMouseMoveBatch(item.events)
    )
    .concat(batch)
}

function clearPointerCapture(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  pointerId: number
) {
  const canvas = canvasRef.current
  if (canvas?.hasPointerCapture(pointerId))
    canvas.releasePointerCapture(pointerId)
}

function releaseEvent(pressed: BrowserPressedPointer): BrowserInputEvent {
  return {
    kind: "mouse",
    eventType: "mouseReleased",
    x: pressed.x,
    y: pressed.y,
    button: pressed.button,
    clickCount: 1,
  }
}
