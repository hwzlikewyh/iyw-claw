"use client"

import { useEffect } from "react"
import { browserApi } from "@/lib/browser-api"
import type {
  BrowserFrameSubscriptionSnapshot,
  BrowserGenerations,
} from "@/lib/browser-types"

const RESIZE_DEBOUNCE_MS = 120
const MIN_VIEWPORT_WIDTH = 320
const MIN_VIEWPORT_HEIGHT = 240
const MAX_VIEWPORT_EDGE = 4096
const STREAM_MAX_WIDTH = 4096
const STREAM_MAX_HEIGHT = 2560
const MIN_STREAM_SCALE = 0.5
const MAX_STREAM_SCALE = 3

type ViewportRequest = {
  width: number
  height: number
  scale: number
}

export function useBrowserViewport(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  tabId: string | undefined,
  claimId: string | undefined,
  subscription: BrowserFrameSubscriptionSnapshot | null,
  generations: BrowserGenerations | null
) {
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !tabId || claimId || !subscription || !generations) return
    let disposed = false
    let timer: number | null = null
    let sending = false
    let pending: ViewportRequest | null = null
    let lastRequest: ViewportRequest | null = null

    const flush = async () => {
      if (sending) return
      sending = true
      try {
        while (!disposed && pending) {
          const request = pending
          pending = null
          await browserApi
            .resize(
              tabId,
              generations,
              request.width,
              request.height,
              request.scale
            )
            .catch(() => {})
        }
      } finally {
        sending = false
      }
    }

    const resize = () => {
      if (timer !== null) window.clearTimeout(timer)
      timer = window.setTimeout(() => {
        const request = viewportRequest(canvas.getBoundingClientRect())
        canvas.dataset.browserViewportWidth = String(request.width)
        canvas.dataset.browserViewportHeight = String(request.height)
        if (sameViewport(lastRequest, request)) return
        lastRequest = request
        pending = request
        void flush()
      }, RESIZE_DEBOUNCE_MS)
    }
    const observer = new ResizeObserver(resize)
    observer.observe(canvas)
    window.addEventListener("resize", resize)
    resize()
    return () => {
      disposed = true
      observer.disconnect()
      window.removeEventListener("resize", resize)
      if (timer !== null) window.clearTimeout(timer)
      delete canvas.dataset.browserViewportWidth
      delete canvas.dataset.browserViewportHeight
    }
  }, [canvasRef, claimId, generations, subscription, tabId])
}

function viewportRequest(rect: DOMRect): ViewportRequest {
  let width = Math.max(1, rect.width)
  let height = Math.max(1, rect.height)
  const grow = Math.max(
    1,
    MIN_VIEWPORT_WIDTH / width,
    MIN_VIEWPORT_HEIGHT / height
  )
  width *= grow
  height *= grow
  const shrink = Math.min(
    1,
    MAX_VIEWPORT_EDGE / width,
    MAX_VIEWPORT_EDGE / height
  )
  width = Math.round(width * shrink)
  height = Math.round(height * shrink)
  const rawScale = Math.max(
    MIN_STREAM_SCALE,
    Math.min(
      MAX_STREAM_SCALE,
      window.devicePixelRatio || 1,
      STREAM_MAX_WIDTH / width,
      STREAM_MAX_HEIGHT / height
    )
  )
  return {
    width,
    height,
    scale: Math.max(MIN_STREAM_SCALE, Math.floor(rawScale * 100) / 100),
  }
}

function sameViewport(
  left: ViewportRequest | null,
  right: ViewportRequest
): boolean {
  return (
    left?.width === right.width &&
    left.height === right.height &&
    left.scale === right.scale
  )
}
