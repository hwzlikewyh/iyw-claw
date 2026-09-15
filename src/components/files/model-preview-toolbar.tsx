"use client"

import { useTranslations } from "next-intl"
import { Pause, Play, RotateCcw } from "lucide-react"
import { PreviewAction } from "./preview-action"
import { PreviewToolbar } from "./preview-toolbar"

interface ModelToolbarProps {
  zoom: number
  playing: boolean
  hasAnimations: boolean
  onZoom: (zoom: number) => void
  onReset: () => void
  onPlay: () => void
  onFullscreen: () => void
}

export function ModelPreviewToolbar(props: ModelToolbarProps) {
  const labels = useTranslations("Folder.previewControls")
  const controls = useTranslations("Folder.fileWorkspacePanel")
  return (
    <PreviewToolbar
      zoom={props.zoom}
      onZoom={props.onZoom}
      onFullscreen={props.onFullscreen}
    >
      <PreviewAction label={controls("imageZoomReset")} onClick={props.onReset}>
        <RotateCcw className="size-4" />
      </PreviewAction>
      {props.hasAnimations && (
        <PreviewAction
          label={labels(props.playing ? "pause" : "play")}
          onClick={props.onPlay}
        >
          {props.playing ? (
            <Pause className="size-4" />
          ) : (
            <Play className="size-4" />
          )}
        </PreviewAction>
      )}
    </PreviewToolbar>
  )
}
