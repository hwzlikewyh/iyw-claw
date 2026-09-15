"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { PreviewToolbar } from "./preview-toolbar"
import { usePreviewVisibility } from "./use-preview-resource"
import { useMediaSource } from "./use-media-source"

const PLAYBACK_RATES = [0.5, 1, 1.25, 1.5, 2]

export function MediaPreview({
  src,
  kind,
  title,
}: {
  src: string
  kind: "video" | "audio"
  title: string
}) {
  const { ref, visible } = usePreviewVisibility()
  const [zoom, setZoom] = useState(1)
  const [rate, setRate] = useState(1)
  const mediaRef = useMediaSource(src, visible, rate)
  const [failed, setFailed] = useState(false)
  const t = useTranslations("Folder.chat.workspaceFiles")
  const labels = useTranslations("Folder.previewControls")
  const Element = kind === "video" ? "video" : "audio"
  return (
    <div ref={ref} className="flex h-full min-h-0 flex-col bg-background">
      <PreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        onFullscreen={() =>
          void ref.current?.requestFullscreen().catch(() => {})
        }
      >
        <select
          aria-label={labels("speed")}
          value={rate}
          onChange={(event) => setRate(Number(event.target.value))}
          className="h-7 bg-background text-xs"
        >
          {PLAYBACK_RATES.map((value) => (
            <option key={value} value={value}>
              {value}x
            </option>
          ))}
        </select>
      </PreviewToolbar>
      {failed && (
        <p role="alert" className="p-3 text-sm text-destructive">
          {t("previewError")}
        </p>
      )}
      <div className="flex min-h-0 flex-1 overflow-auto bg-black/5">
        <div
          className="m-auto flex min-h-full shrink-0 items-center justify-center"
          style={{ width: `${zoom * 100}%` }}
        >
          <Element
            ref={mediaRef}
            controls
            playsInline
            preload="metadata"
            aria-label={title}
            onError={() => setFailed(true)}
            className="max-h-full w-full"
          />
        </div>
      </div>
    </div>
  )
}
