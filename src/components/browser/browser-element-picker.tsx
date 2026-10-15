"use client"

import { useEffect, useRef, useState, type PointerEvent } from "react"
import { useTranslations } from "next-intl"
import { browserApi } from "@/lib/browser-api"
import type { BrowserElementSummary } from "@/lib/browser-types"

const HOVER_INTERVAL_MS = 80
type Point = { x: number; y: number }

export function BrowserElementPicker({
  tabId,
  onPick,
  onCancel,
}: {
  tabId: string
  onPick: (element: BrowserElementSummary) => Promise<void>
  onCancel: () => void
}) {
  const t = useTranslations("Browser")
  const [element, setElement] = useState<BrowserElementSummary | null>(null)
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)
  const point = useRef<Point | null>(null)
  const pending = useRef<Promise<BrowserElementSummary> | null>(null)
  const picking = useRef(false)
  const latest = useRef({ onCancel, onPick })
  const active = useRef(true)
  useEffect(() => {
    latest.current = { onCancel, onPick }
  }, [onCancel, onPick])
  useEffect(() => {
    let disposed = false
    active.current = true
    const timer = window.setInterval(() => {
      const next = point.current
      if (!next || pending.current || picking.current) return
      point.current = null
      const request = browserApi.inspectElement(tabId, next.x, next.y)
      pending.current = request
      void request
        .then((value) => {
          if (!disposed && !picking.current) setElement(value)
        })
        .catch(() => {
          if (!disposed) setElement(null)
        })
        .finally(() => {
          if (pending.current === request) pending.current = null
        })
    }, HOVER_INTERVAL_MS)
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return
      event.preventDefault()
      latest.current.onCancel()
    }
    window.addEventListener("keydown", escape, true)
    return () => {
      disposed = true
      active.current = false
      window.clearInterval(timer)
      window.removeEventListener("keydown", escape, true)
    }
  }, [tabId])

  const select = async (position: Point) => {
    if (picking.current) return
    picking.current = true
    setBusy(true)
    setFailed(false)
    try {
      await pending.current?.catch(() => {})
      if (!active.current) return
      const selected = await browserApi.inspectElement(
        tabId,
        position.x,
        position.y
      )
      if (!active.current) return
      await latest.current.onPick(selected)
    } catch {
      if (active.current) setFailed(true)
    } finally {
      picking.current = false
      if (active.current) setBusy(false)
    }
  }
  const move = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    event.stopPropagation()
    const position = surfacePoint(event, element?.viewport)
    point.current = position
    return position
  }
  const bounds = element?.bounds
  const viewport = element?.viewport
  return (
    <div
      className="absolute inset-0 z-10 cursor-crosshair overflow-hidden touch-none"
      role="region"
      aria-label={t("selectElement")}
      onPointerMove={move}
      onPointerDown={(event) => {
        event.preventDefault()
        event.stopPropagation()
      }}
      onPointerUp={(event) => {
        if (event.button === 0) void select(move(event))
      }}
      onWheel={(event) => {
        event.preventDefault()
        event.stopPropagation()
      }}
      onContextMenu={(event) => {
        event.preventDefault()
        latest.current.onCancel()
      }}
    >
      {bounds && viewport && viewport[0] > 0 && viewport[1] > 0 ? (
        <div
          className="pointer-events-none absolute border border-violet-500 bg-violet-500/15"
          style={{
            left: `${(bounds[0] / viewport[0]) * 100}%`,
            top: `${(bounds[1] / viewport[1]) * 100}%`,
            width: `${(bounds[2] / viewport[0]) * 100}%`,
            height: `${(bounds[3] / viewport[1]) * 100}%`,
          }}
        >
          <span className="absolute left-0 top-0 max-w-80 truncate bg-violet-600 px-1 text-[11px] text-white">
            {element.selector} {Math.round(bounds[2])}×{Math.round(bounds[3])}
          </span>
        </div>
      ) : null}
      <div
        className="pointer-events-none absolute bottom-2 left-1/2 -translate-x-1/2 rounded-md bg-background/95 px-3 py-1.5 text-xs shadow"
        role="status"
      >
        {failed
          ? t("elementFailed")
          : busy
            ? t("selectionCapturing")
            : t("selectionHint")}
      </div>
    </div>
  )
}

function surfacePoint(
  event: PointerEvent<HTMLDivElement>,
  viewport?: [number, number]
): Point {
  const overlay = event.currentTarget
  const rect = overlay.getBoundingClientRect()
  const canvas = overlay.parentElement?.querySelector("canvas")
  const width =
    viewport?.[0] || Number(canvas?.dataset.browserViewportWidth) || rect.width
  const height =
    viewport?.[1] ||
    Number(canvas?.dataset.browserViewportHeight) ||
    rect.height
  return {
    x: Math.max(
      0,
      Math.min(
        width,
        ((event.clientX - rect.left) / Math.max(1, rect.width)) * width
      )
    ),
    y: Math.max(
      0,
      Math.min(
        height,
        ((event.clientY - rect.top) / Math.max(1, rect.height)) * height
      )
    ),
  }
}
