"use client"

import { useEffect, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { ModelPreviewToolbar } from "./model-preview-toolbar"
import { usePreviewVisibility } from "./use-preview-resource"
import { useModelPreview } from "./use-model-preview"

export function ModelPreview({ src, title }: { src: string; title: string }) {
  const { ref, visible } = usePreviewVisibility()
  const [zoom, setZoom] = useState(1)
  const [playing, setPlaying] = useState(false)
  const { sceneRef, hostRef, status, hasAnimations } = useModelPreview(
    src,
    visible
  )
  const labels = useTranslations("Folder.previewControls")
  useEffect(() => {
    sceneRef.current?.zoom(zoom)
  }, [zoom, status, sceneRef])
  useEffect(() => {
    sceneRef.current?.play(playing && visible)
  }, [playing, visible, status, sceneRef])
  return (
    <div
      ref={ref}
      className="relative flex h-full min-h-0 flex-col"
      aria-label={title}
    >
      <ModelPreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        playing={playing}
        hasAnimations={hasAnimations}
        onReset={() => {
          setZoom(1)
          sceneRef.current?.reset()
        }}
        onPlay={() => setPlaying((value) => !value)}
        onFullscreen={() =>
          void ref.current?.requestFullscreen().catch(() => {})
        }
      />
      <div
        ref={hostRef}
        className="min-h-0 flex-1 overflow-hidden [&_canvas]:block"
      />
      {status === "loading" && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <Loader2 className="size-5 animate-spin" />
        </div>
      )}
      {status === "error" && (
        <p
          role="alert"
          className="absolute inset-x-0 top-12 p-4 text-sm text-destructive"
        >
          {labels("modelLimit")}
        </p>
      )}
    </div>
  )
}
