"use client"

import { Maximize2, Minus, Plus, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { createContext, useContext, type ReactNode } from "react"
import { PreviewAction } from "./preview-action"

export const PREVIEW_ZOOM_MIN = 0.5
export const PREVIEW_ZOOM_MAX = 3
export const PREVIEW_ZOOM_STEP = 0.25

export const PreviewFullscreenContext = createContext(false)

export function PreviewToolbar({
  zoom,
  onZoom,
  onFullscreen,
  children,
}: {
  zoom: number
  onZoom: (zoom: number) => void
  onFullscreen?: () => void
  children?: ReactNode
}) {
  const full = useTranslations("Folder.taskArtifacts")
  const hasParentFullscreen = useContext(PreviewFullscreenContext)
  return (
    <div className="flex min-h-10 shrink-0 flex-wrap items-center gap-2 border-b bg-background px-2 py-1">
      <PreviewZoomControls zoom={zoom} onZoom={onZoom} />
      {children && (
        <div className="flex shrink-0 items-center gap-1 border-l pl-2">
          {children}
        </div>
      )}
      {onFullscreen && !hasParentFullscreen && (
        <PreviewAction
          className="ml-auto"
          label={full("previewFullscreen")}
          onClick={onFullscreen}
        >
          <Maximize2 className="size-4" />
        </PreviewAction>
      )}
    </div>
  )
}

function PreviewZoomControls({
  zoom,
  onZoom,
}: {
  zoom: number
  onZoom: (zoom: number) => void
}) {
  const t = useTranslations("Folder.fileWorkspacePanel")
  const change = (delta: number) =>
    onZoom(Math.max(PREVIEW_ZOOM_MIN, Math.min(PREVIEW_ZOOM_MAX, zoom + delta)))
  return (
    <div className="flex shrink-0 items-center gap-1">
      <PreviewAction
        label={t("imageZoomOut")}
        disabled={zoom <= PREVIEW_ZOOM_MIN}
        onClick={() => change(-PREVIEW_ZOOM_STEP)}
      >
        <Minus className="size-4" />
      </PreviewAction>
      <output className="w-12 text-center text-xs tabular-nums">
        {Math.round(zoom * 100)}%
      </output>
      <PreviewAction
        label={t("imageZoomIn")}
        disabled={zoom >= PREVIEW_ZOOM_MAX}
        onClick={() => change(PREVIEW_ZOOM_STEP)}
      >
        <Plus className="size-4" />
      </PreviewAction>
      <PreviewAction label={t("imageZoomReset")} onClick={() => onZoom(1)}>
        <RotateCcw className="size-4" />
      </PreviewAction>
    </div>
  )
}
