"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
  acknowledgeBrowserFrame,
  browserApi,
  commitBrowserClaim,
  getBrowserFrameSubscription,
  subscribeBrowserFrames,
  unsubscribeBrowserFrames,
} from "@/lib/browser-api"
import { parseBrowserFrame } from "@/lib/browser-frame"
import type {
  BrowserFrameSubscriptionSnapshot,
  BrowserTabSnapshot,
  BrowserViewClaimSnapshot,
} from "@/lib/browser-types"
import { useBrowser } from "@/contexts/browser-context"

const RESIZE_DEBOUNCE_MS = 150
const STATUS_POLL_MS = 1_500
const RETRY_DELAY_MS = 750

export function useBrowserFrames(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  tab: BrowserTabSnapshot | null,
  claim?: BrowserViewClaimSnapshot
) {
  const { acceptState, refresh } = useBrowser()
  const [subscription, setSubscription] =
    useState<BrowserFrameSubscriptionSnapshot | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [retryKey, setRetryKey] = useState(0)
  const [streamIdentity, setStreamIdentity] = useState<{
    owner: string
    generations: BrowserFrameSubscriptionSnapshot["generations"]
  } | null>(null)
  const committedRef = useRef(false)
  const tabId = tab?.browserTabId
  const tabStatus = tab?.status
  const claimId = claim?.claimId
  const sourceGenerations = claim?.generations ?? tab?.generations
  const runtimeGeneration = sourceGenerations?.runtimeGeneration
  const tabGeneration = sourceGenerations?.tabGeneration
  const viewGeneration = sourceGenerations?.viewGeneration
  const currentGenerations = useMemo(
    () =>
      runtimeGeneration !== undefined &&
      tabGeneration !== undefined &&
      viewGeneration !== undefined
        ? {
            runtimeGeneration,
            tabGeneration,
            viewGeneration,
            // 控制权代次随每次用户输入变化，不属于帧流身份。
            controlEpoch: 0,
          }
        : null,
    [runtimeGeneration, tabGeneration, viewGeneration]
  )
  const owner = `${tabId ?? "none"}:${claimId ?? "direct"}`
  useEffect(() => {
    if (!currentGenerations || tabStatus !== "live") return
    setStreamIdentity({ owner, generations: currentGenerations })
  }, [currentGenerations, owner, tabStatus])
  const generations =
    tabStatus === "navigating" && streamIdentity?.owner === owner
      ? streamIdentity.generations
      : currentGenerations
  const canStream = tabStatus === "live" || tabStatus === "navigating"

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !tabId || !canStream || !generations) return
    let disposed = false
    let active: BrowserFrameSubscriptionSnapshot | null = null
    let queued: ArrayBuffer | Uint8Array | number[] | null = null
    let drawing = false
    let retryTimer: number | null = null
    const context = canvas.getContext("2d", {
      alpha: false,
      desynchronized: true,
    })
    committedRef.current = false

    const retry = () => {
      if (disposed || retryTimer !== null) return
      setSubscription(null)
      retryTimer = window.setTimeout(() => {
        retryTimer = null
        if (!disposed) setRetryKey((value) => value + 1)
      }, RETRY_DELAY_MS)
    }

    const draw = async (raw: ArrayBuffer | Uint8Array | number[]) => {
      if (disposed || drawing || !active) {
        queued = raw
        return
      }
      drawing = true
      try {
        const frame = parseBrowserFrame(raw)
        if (!matchesFrame(frame, generations)) {
          throw new Error("Browser frame generation changed")
        }
        const bitmap = await createImageBitmap(
          new Blob([frame.jpeg.slice().buffer], { type: "image/jpeg" })
        )
        if (disposed) {
          bitmap.close()
          return
        }
        if (canvas.width !== frame.width) canvas.width = frame.width
        if (canvas.height !== frame.height) canvas.height = frame.height
        context?.drawImage(bitmap, 0, 0, frame.width, frame.height)
        bitmap.close()
        await acknowledgeBrowserFrame(
          active.subscriptionId,
          generations,
          frame.seq,
          claimId
        )
        if (claimId && !committedRef.current) {
          committedRef.current = true
          acceptState(
            await commitBrowserClaim(
              claimId,
              active.subscriptionId,
              generations
            )
          )
        }
        setError(null)
      } catch (cause) {
        if (!disposed) {
          setError(String(cause))
          void refresh()
          retry()
        }
      } finally {
        drawing = false
        const next = queued
        queued = null
        if (next && !disposed) void draw(next)
      }
    }

    void subscribeBrowserFrames(
      tabId,
      generations,
      claimId,
      (raw) => void draw(raw)
    )
      .then((result) => {
        if (disposed) {
          return unsubscribeBrowserFrames(
            result.subscription.subscriptionId,
            generations
          )
        }
        active = result.subscription
        setSubscription(result.subscription)
        if (queued) {
          const next = queued
          queued = null
          void draw(next)
        }
      })
      .catch((cause) => {
        if (!disposed) {
          setError(String(cause))
          retry()
        }
      })

    return () => {
      disposed = true
      if (retryTimer !== null) window.clearTimeout(retryTimer)
      setSubscription(null)
      if (active) {
        void unsubscribeBrowserFrames(
          active.subscriptionId,
          active.generations
        ).catch(() => {})
      }
    }
  }, [
    acceptState,
    canvasRef,
    claimId,
    canStream,
    generations,
    refresh,
    retryKey,
    tabId,
  ])

  useEffect(() => {
    if (!subscription) return
    let disposed = false
    let checking = false
    let retryTimer: number | null = null
    const check = async () => {
      if (checking || disposed) return
      checking = true
      try {
        const current = await getBrowserFrameSubscription(
          subscription.subscriptionId,
          subscription.generations
        )
        if (disposed) return
        if (current.status !== "disconnected") {
          if (current.status !== subscription.status) setSubscription(current)
          return
        }
        setError("Browser stream disconnected")
      } catch (cause) {
        if (!disposed) setError(String(cause))
      } finally {
        checking = false
      }
      if (!disposed) {
        setSubscription(null)
        retryTimer = window.setTimeout(() => {
          if (!disposed) setRetryKey((value) => value + 1)
        }, RETRY_DELAY_MS)
      }
    }
    const timer = window.setInterval(() => void check(), STATUS_POLL_MS)
    return () => {
      disposed = true
      window.clearInterval(timer)
      if (retryTimer !== null) window.clearTimeout(retryTimer)
    }
  }, [subscription])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || !tabId || claimId || !subscription || !generations) return
    let timer: number | null = null
    const resize = () => {
      if (timer !== null) window.clearTimeout(timer)
      timer = window.setTimeout(() => {
        const rect = canvas.getBoundingClientRect()
        if (rect.width < 320 || rect.height < 240) return
        void browserApi
          .resize(
            tabId,
            generations,
            Math.round(rect.width),
            Math.round(rect.height),
            Math.min(3, Math.max(0.5, window.devicePixelRatio || 1))
          )
          .catch(() => {})
      }, RESIZE_DEBOUNCE_MS)
    }
    const observer = new ResizeObserver(resize)
    observer.observe(canvas)
    resize()
    return () => {
      observer.disconnect()
      if (timer !== null) window.clearTimeout(timer)
    }
  }, [canvasRef, claimId, generations, subscription, tabId])

  return { subscription, error }
}

function matchesFrame(
  frame: {
    runtimeGeneration: number
    tabGeneration: number
    viewGeneration: number
  },
  expected: BrowserTabSnapshot["generations"]
) {
  return (
    frame.runtimeGeneration === expected.runtimeGeneration &&
    frame.tabGeneration === expected.tabGeneration &&
    frame.viewGeneration === expected.viewGeneration
  )
}
