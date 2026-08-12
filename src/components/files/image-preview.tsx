"use client"

import { useMemo, useState } from "react"
import { useTranslations } from "next-intl"

import {
  useImageViewport,
  useImageZoom,
  useRightButtonPan,
} from "@/components/files/image-preview-interactions"
import { ImagePreviewToolbar } from "@/components/files/image-preview-toolbar"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import type { FileWorkspaceTab } from "@/contexts/workspace-context"

const IMAGE_PADDING = 48

export function ImagePreview({ tab }: { tab: FileWorkspaceTab }) {
  const t = useTranslations("Folder.fileWorkspacePanel")
  const [naturalSize, setNaturalSize] = useState({ width: 0, height: 0 })
  const [editorOpen, setEditorOpen] = useState(false)
  const zoom = useImageZoom()
  const viewport = useImageViewport(zoom.setZoom)
  const pan = useRightButtonPan(viewport.elementRef)
  const fileSize = useMemo(() => imageByteLength(tab.content), [tab.content])
  const displaySize = fittedSize({
    naturalSize,
    viewportSize: viewport.size,
    zoom: zoom.zoom,
  })
  return (
    <div className="flex h-full flex-col">
      {tab.loading ? <LoadingBadge label={t("loading")} /> : null}
      {tab.content ? (
        <>
          <ImagePreviewToolbar
            zoom={zoom.zoom}
            naturalWidth={naturalSize.width}
            naturalHeight={naturalSize.height}
            fileSize={fileSize}
            onZoomIn={zoom.zoomIn}
            onZoomOut={zoom.zoomOut}
            onZoomReset={zoom.resetZoom}
            onEdit={() => setEditorOpen(true)}
          />
          <ImageViewport
            src={tab.content}
            alt={tab.title}
            displaySize={displaySize}
            viewportRef={viewport.viewportRef}
            onLoad={setNaturalSize}
            {...pan}
          />
          <ImagePreviewDialog
            src={tab.content}
            alt={tab.title}
            open={editorOpen && naturalSize.width > 0 && naturalSize.height > 0}
            onOpenChange={setEditorOpen}
          />
        </>
      ) : tab.loading ? (
        <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
          {t("loading")}
        </div>
      ) : null}
    </div>
  )
}

function LoadingBadge({ label }: { label: string }) {
  return (
    <div className="absolute right-3 top-2 z-10 rounded-md bg-background/70 px-2 py-1 text-[11px] text-muted-foreground backdrop-blur-sm">
      {label}
    </div>
  )
}

interface DisplaySize {
  width?: number
  height?: number
}

function ImageViewport({
  src,
  alt,
  displaySize,
  viewportRef,
  onLoad,
  onMouseDown,
  onContextMenu,
}: {
  src: string
  alt: string
  displaySize: DisplaySize
  viewportRef: React.RefCallback<HTMLDivElement>
  onLoad: (size: { width: number; height: number }) => void
  onMouseDown: (event: React.MouseEvent) => void
  onContextMenu: (event: React.MouseEvent) => void
}) {
  return (
    <div
      ref={viewportRef}
      className="min-h-0 flex-1 overflow-auto bg-[repeating-conic-gradient(hsl(var(--muted))_0%_25%,transparent_0%_50%)] bg-[length:16px_16px]"
      onMouseDown={onMouseDown}
      onContextMenu={onContextMenu}
    >
      <ImageCanvas
        src={src}
        alt={alt}
        displaySize={displaySize}
        onLoad={onLoad}
      />
    </div>
  )
}

function ImageCanvas({
  src,
  alt,
  displaySize,
  onLoad,
}: Pick<
  Parameters<typeof ImageViewport>[0],
  "src" | "alt" | "displaySize" | "onLoad"
>) {
  const sized = displaySize.width != null
  return (
    <div
      className="box-border flex min-h-full min-w-full items-center justify-center p-6"
      style={
        sized
          ? {
              width: displaySize.width! + IMAGE_PADDING,
              height: displaySize.height! + IMAGE_PADDING,
            }
          : undefined
      }
    >
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={src}
        alt={alt}
        onLoad={(event) =>
          onLoad({
            width: event.currentTarget.naturalWidth,
            height: event.currentTarget.naturalHeight,
          })
        }
        className="block shrink-0"
        style={
          sized
            ? { width: displaySize.width, height: displaySize.height }
            : { maxWidth: "100%", maxHeight: "100%" }
        }
      />
    </div>
  )
}

function fittedSize({
  naturalSize,
  viewportSize,
  zoom,
}: {
  naturalSize: { width: number; height: number }
  viewportSize: { width: number; height: number }
  zoom: number
}): DisplaySize {
  if (!naturalSize.width || !naturalSize.height) return {}
  const width = viewportSize.width - IMAGE_PADDING
  const height = viewportSize.height - IMAGE_PADDING
  if (width <= 0 || height <= 0) return {}
  const fit = Math.min(
    1,
    width / naturalSize.width,
    height / naturalSize.height
  )
  return {
    width: Math.round(naturalSize.width * fit) * zoom,
    height: Math.round(naturalSize.height * fit) * zoom,
  }
}

function imageByteLength(content: string | null): number {
  const base64 = content?.split(",")[1]
  if (!base64) return 0
  const padding = (base64.match(/=+$/) ?? [""])[0].length
  return Math.floor((base64.length * 3) / 4) - padding
}
