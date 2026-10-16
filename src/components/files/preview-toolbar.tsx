"use client"

import { Maximize2, Minimize2, Minus, Plus, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { createContext, useContext, type ReactNode } from "react"
import { PreviewAction } from "./preview-action"

export const PREVIEW_ZOOM_MIN = 0.5
export const PREVIEW_ZOOM_MAX = 3
export const PREVIEW_ZOOM_STEP = 0.25

export const PreviewFullscreenContext = createContext(false)

interface PreviewToolbarProps {
  zoom: number
  onZoom: (zoom: number) => void
  onFullscreen?: () => void
  isFullscreen?: boolean
  compact?: boolean
  children?: ReactNode
}

export function PreviewToolbar({
  zoom,
  onZoom,
  onFullscreen,
  isFullscreen = false,
  compact = false,
  children,
}: PreviewToolbarProps) {
  return (
    <div
      className={
        compact
          ? "absolute top-2 right-2 z-10 rounded-md bg-background/90 shadow-sm"
          : "flex min-h-10 shrink-0 flex-wrap items-center gap-2 border-b bg-background px-2 py-1"
      }
    >
      {!compact && <PreviewZoomControls zoom={zoom} onZoom={onZoom} />}
      {!compact && children && (
        <div className="flex shrink-0 items-center gap-1 border-l pl-2">
          {children}
        </div>
      )}
      <PreviewFullscreenAction
        onFullscreen={onFullscreen}
        isFullscreen={isFullscreen}
      />
    </div>
  )
}

function PreviewFullscreenAction({
  onFullscreen,
  isFullscreen,
}: Pick<PreviewToolbarProps, "onFullscreen" | "isFullscreen">) {
  const full = useTranslations("Folder.taskArtifacts")
  const hasParentFullscreen = useContext(PreviewFullscreenContext)
  if (!onFullscreen || hasParentFullscreen) return null
  return (
    <PreviewAction
      className="ml-auto"
      label={full(isFullscreen ? "exitPreviewFullscreen" : "previewFullscreen")}
      aria-pressed={Boolean(isFullscreen)}
      onClick={onFullscreen}
    >
      {isFullscreen ? (
        <Minimize2 className="size-4" />
      ) : (
        <Maximize2 className="size-4" />
      )}
    </PreviewAction>
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
