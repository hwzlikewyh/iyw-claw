"use client"

import { useState, type RefObject } from "react"
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
  return (
    <div ref={ref} className="flex h-full min-h-0 flex-col bg-background">
      <PreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        onFullscreen={() =>
          void ref.current?.requestFullscreen().catch(() => {})
        }
      >
        <PlaybackSpeed rate={rate} onChange={setRate} />
      </PreviewToolbar>
      {failed && (
        <p role="alert" className="p-3 text-sm text-destructive">
          {t("previewError")}
        </p>
      )}
      <MediaViewport
        kind={kind}
        title={title}
        zoom={zoom}
        mediaRef={mediaRef}
        onError={() => setFailed(true)}
      />
    </div>
  )
}

function MediaViewport({
  kind,
  title,
  zoom,
  mediaRef,
  onError,
}: {
  kind: "video" | "audio"
  title: string
  zoom: number
  mediaRef: RefObject<(HTMLVideoElement & HTMLAudioElement) | null>
  onError: () => void
}) {
  const Element = kind === "video" ? "video" : "audio"
  return (
    <div
      className={`flex min-h-0 min-w-0 flex-1 overflow-auto ${kind === "video" ? "bg-black" : "bg-muted/20"}`}
    >
      <div
        className="m-auto flex shrink-0 items-center justify-center"
        style={{ width: `${zoom * 100}%`, height: `${zoom * 100}%` }}
      >
        <Element
          ref={mediaRef}
          controls
          playsInline
          preload="metadata"
          aria-label={title}
          onError={onError}
          className={
            kind === "video"
              ? "block size-full object-contain"
              : "w-full max-w-xl"
          }
        />
      </div>
    </div>
  )
}

function PlaybackSpeed({
  rate,
  onChange,
}: {
  rate: number
  onChange: (rate: number) => void
}) {
  const labels = useTranslations("Folder.previewControls")
  return (
    <select
      aria-label={labels("speed")}
      title={labels("speed")}
      value={rate}
      onChange={(event) => onChange(Number(event.target.value))}
      className="h-7 w-16 shrink-0 rounded-sm bg-background px-1 text-xs tabular-nums"
    >
      {PLAYBACK_RATES.map((value) => (
        <option key={value} value={value}>
          {value}x
        </option>
      ))}
    </select>
  )
}
