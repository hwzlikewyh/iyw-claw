"use client"

import { useEffect, useRef, useState, type MouseEvent } from "react"
import { browserApi } from "@/lib/browser-api"
import type { BrowserScreenshot, BrowserTabSnapshot } from "@/lib/browser-types"

const VISIBILITY_POLL_MS = 250
const queues = new Map<string, Promise<unknown>>()

function enqueue(tabId: string, operation: () => Promise<unknown>) {
  const next = (queues.get(tabId) ?? Promise.resolve()).then(
    operation,
    operation
  )
  queues.set(tabId, next)
  void next
    .finally(() => {
      if (queues.get(tabId) === next) queues.delete(tabId)
    })
    .catch(() => {})
  return next
}

function obstructed(element: HTMLDivElement) {
  if (
    document.visibilityState !== "visible" ||
    !element.getClientRects().length
  )
    return true
  if (element.closest('[inert], [aria-hidden="true"]')) return true
  return Array.from(
    document.querySelectorAll<HTMLElement>(
      '[role="dialog"], [role="alertdialog"], [role="menu"], [data-slot="popover-content"], [data-slot="dropdown-menu-content"]'
    )
  ).some(
    (overlay) =>
      overlay.getClientRects().length > 0 &&
      getComputedStyle(overlay).visibility !== "hidden" &&
      overlay.getAttribute("aria-hidden") !== "true"
  )
}

export function BrowserNativeSurface({
  tab,
  selecting = false,
  onSelectPoint,
  onSelectionFrame,
}: {
  tab: BrowserTabSnapshot
  selecting?: boolean
  onSelectPoint?: (point: { x: number; y: number }) => void
  onSelectionFrame?: (frame: BrowserScreenshot | null) => void
}) {
  const container = useRef<HTMLDivElement>(null)
  const [frame, setFrame] = useState<BrowserScreenshot | null>(null)
  const [previousSelecting, setPreviousSelecting] = useState(selecting)
  if (previousSelecting !== selecting) {
    setPreviousSelecting(selecting)
    setFrame(null)
  }
  const [error, setError] = useState<string | null>(null)
  const tabId = tab.browserTabId
  useNativePlacement(container, tabId, selecting && frame !== null, setError)
  useEffect(() => {
    if (!selecting) {
      onSelectionFrame?.(null)
      return
    }
    let disposed = false
    onSelectionFrame?.(null)
    void browserApi
      .captureScreenshot(tabId)
      .then(async (value) => {
        if (disposed) return
        await enqueue(tabId, () =>
          browserApi.nativeView({ tabId, visible: false })
        )
        if (!disposed) {
          setFrame(value)
          onSelectionFrame?.(value)
        }
      })
      .catch((cause) => {
        if (!disposed) setError(String(cause))
      })
    return () => {
      disposed = true
    }
  }, [tabId, selecting, onSelectionFrame])
  const select = (event: MouseEvent<HTMLDivElement>) => {
    if (!selecting || !frame) return
    const rect = event.currentTarget.getBoundingClientRect()
    onSelectPoint?.({
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    })
  }
  return (
    <div
      ref={container}
      onClick={select}
      className={`relative h-full min-h-0 w-full bg-background ${selecting ? "cursor-crosshair" : ""}`}
      data-native-browser-surface={tabId}
    >
      {selecting && frame ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={`data:${frame.mimeType};base64,${frame.data}`}
          alt=""
          draggable={false}
          className="pointer-events-none h-full w-full object-fill"
        />
      ) : null}
      {error ? (
        <div
          className="absolute inset-x-0 bottom-0 bg-destructive p-2 text-xs text-destructive-foreground"
          role="status"
        >
          {error}
        </div>
      ) : null}
    </div>
  )
}

function useNativePlacement(
  ref: React.RefObject<HTMLDivElement | null>,
  tabId: string,
  selecting: boolean,
  onError: (message: string | null) => void
) {
  useEffect(() => {
    const element = ref.current
    if (!element) return
    let disposed = false
    let pending = false
    let dirty = false
    let last = ""
    const sync = async () => {
      if (disposed) return
      if (pending) {
        dirty = true
        return
      }
      dirty = false
      const rect = element.getBoundingClientRect()
      const bounds = {
        x: rect.left,
        y: rect.top,
        width: rect.width,
        height: rect.height,
      }
      const visible =
        !selecting && !obstructed(element) && rect.width > 0 && rect.height > 0
      const key = visible ? JSON.stringify(bounds) : "hidden"
      if (key === last) return
      pending = true
      try {
        await enqueue(tabId, () =>
          browserApi.nativeView({
            tabId,
            visible,
            ...(visible ? { bounds } : {}),
          })
        )
        last = key
        if (!disposed) onError(null)
      } catch (cause) {
        if (!disposed)
          onError(String((cause as { message?: string })?.message ?? cause))
      } finally {
        pending = false
        if (dirty && !disposed) void sync()
      }
    }
    const observer = new ResizeObserver(() => void sync())
    const mutations = new MutationObserver(() => void sync())
    observer.observe(element)
    mutations.observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["data-state", "inert", "aria-hidden", "class"],
    })
    const timer = window.setInterval(() => void sync(), VISIBILITY_POLL_MS)
    void sync()
    return () => {
      disposed = true
      clearInterval(timer)
      observer.disconnect()
      mutations.disconnect()
      void enqueue(tabId, () =>
        browserApi.nativeView({ tabId, visible: false })
      ).catch(() => {})
    }
  }, [ref, tabId, selecting, onError])
}
