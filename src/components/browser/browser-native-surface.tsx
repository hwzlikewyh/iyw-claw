"use client"

import { useRef, type MouseEvent } from "react"
import { useNativeBrowserPlacement } from "@/hooks/use-native-browser-placement"
import type { BrowserScreenshot, BrowserTabSnapshot } from "@/lib/browser-types"

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
  const tabId = tab.browserTabId
  const { frame, error } = useNativeBrowserPlacement({
    container,
    tabId,
    selecting,
    onSelectionFrame,
  })
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
      {frame ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={`data:${frame.mimeType};base64,${frame.data}`}
          alt=""
          draggable={false}
          className="pointer-events-none h-full w-full object-fill"
          data-browser-frozen-frame=""
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
