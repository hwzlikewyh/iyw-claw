"use client"

import { Minus, Pencil, Plus, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"

import { ZOOM_MAX, ZOOM_MIN } from "./image-preview-interactions"

interface ImagePreviewToolbarProps {
  zoom: number
  naturalWidth: number
  naturalHeight: number
  fileSize: number
  onZoomIn: () => void
  onZoomOut: () => void
  onZoomReset: () => void
  onEdit: () => void
}

export function ImagePreviewToolbar(props: ImagePreviewToolbarProps) {
  const t = useTranslations("Folder.fileWorkspacePanel")
  const imageT = useTranslations("Folder.chat.messageList")
  return (
    <div className="flex flex-none items-center gap-1 border-b border-border bg-muted/30 px-3 py-1">
      <IconButton
        label={t("imageZoomOut")}
        disabled={props.zoom <= ZOOM_MIN}
        onClick={props.onZoomOut}
      >
        <Minus className="size-3.5" />
      </IconButton>
      <button
        type="button"
        onClick={props.onZoomReset}
        className="min-w-14 rounded px-1.5 py-0.5 text-center font-mono text-[11px] text-muted-foreground transition-colors hover:bg-muted"
        title={t("imageZoomReset")}
      >
        {Math.round(props.zoom * 100)}%
      </button>
      <IconButton
        label={t("imageZoomIn")}
        disabled={props.zoom >= ZOOM_MAX}
        onClick={props.onZoomIn}
      >
        <Plus className="size-3.5" />
      </IconButton>
      <IconButton label={t("imageZoomReset")} onClick={props.onZoomReset}>
        <RotateCcw className="size-3.5" />
      </IconButton>
      <IconButton
        label={imageT("imageEditorTitle")}
        disabled={props.naturalWidth === 0 || props.naturalHeight === 0}
        onClick={props.onEdit}
      >
        <Pencil className="size-3.5" />
      </IconButton>
      <ImageStats {...props} />
    </div>
  )
}

function IconButton({
  label,
  disabled = false,
  onClick,
  children,
}: {
  label: string
  disabled?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="rounded p-1 transition-colors hover:bg-muted disabled:opacity-40"
      aria-label={label}
      title={label}
    >
      {children}
    </button>
  )
}

function ImageStats({
  naturalWidth,
  naturalHeight,
  fileSize,
}: Pick<
  ImagePreviewToolbarProps,
  "naturalWidth" | "naturalHeight" | "fileSize"
>) {
  return (
    <div className="ml-auto flex items-center gap-3 text-[11px] text-muted-foreground">
      {naturalWidth > 0 && naturalHeight > 0 ? (
        <span>
          {naturalWidth} x {naturalHeight}
        </span>
      ) : null}
      {fileSize > 0 ? <span>{formatFileSize(fileSize)}</span> : null}
    </div>
  )
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
