"use client"

import type { MouseEvent } from "react"
import { useState } from "react"
import { ExternalLink, ImageIcon } from "lucide-react"
import { useTranslations } from "next-intl"

import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import { cn } from "@/lib/utils"

const IMAGE_PATH_PATTERN = /\.(?:avif|bmp|gif|jpe?g|png|svg|webp)$/i

interface ImageDimensions {
  width: number
  height: number
}

interface MarkdownImageLinkProps {
  src: string
  alt: string
  onOpenSource: (event: MouseEvent<HTMLButtonElement>) => void | Promise<void>
}

export function isImageUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return (
      (url.protocol === "http:" || url.protocol === "https:") &&
      IMAGE_PATH_PATTERN.test(url.pathname)
    )
  } catch {
    return false
  }
}

function imageName(src: string, fallback: string): string {
  try {
    const name = decodeURIComponent(
      new URL(src).pathname.split("/").pop() ?? ""
    )
    return name || fallback || src
  } catch {
    return fallback || src
  }
}

export function MarkdownImageLink({
  src,
  alt,
  onOpenSource,
}: MarkdownImageLinkProps) {
  const t = useTranslations("Folder.chat.messageList")
  const [dimensions, setDimensions] = useState<ImageDimensions | null>(null)
  const [failed, setFailed] = useState(false)
  const [previewOpen, setPreviewOpen] = useState(false)
  const accessibleName = imageName(src, alt)

  return (
    <span className="not-prose my-1.5 inline-flex w-64 max-w-full flex-col overflow-hidden rounded-md border border-border/70 bg-muted/20 align-top">
      <button
        type="button"
        onClick={() => setPreviewOpen(true)}
        disabled={failed}
        className="relative aspect-square w-full cursor-zoom-in overflow-hidden bg-muted/30 disabled:cursor-default"
        aria-label={accessibleName}
        title={accessibleName}
      >
        {/* Remote image hosts are dynamic, so next/image cannot declare them. */}
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={src}
          alt={accessibleName}
          onLoad={(event) => {
            setDimensions({
              width: event.currentTarget.naturalWidth,
              height: event.currentTarget.naturalHeight,
            })
            setFailed(false)
          }}
          onError={() => {
            setDimensions(null)
            setFailed(true)
          }}
          className={cn(
            "size-full object-contain transition-opacity",
            !dimensions && "opacity-0"
          )}
        />
        {!dimensions ? (
          <span className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-3 text-xs text-muted-foreground">
            <ImageIcon className="size-6 opacity-60" aria-hidden="true" />
            <span>
              {t(failed ? "imageEditorLoadFailed" : "imageEditorLoading")}
            </span>
          </span>
        ) : null}
      </button>

      <span className="flex min-w-0 items-center justify-between gap-2 border-t border-border/60 px-2.5 py-2 text-xs text-muted-foreground">
        <span className="shrink-0 tabular-nums">
          {dimensions
            ? `${dimensions.width} × ${dimensions.height}px`
            : t(failed ? "imageEditorLoadFailed" : "imageEditorLoading")}
        </span>
        <button
          type="button"
          onClick={onOpenSource}
          title={src}
          className="inline-flex min-w-0 items-center gap-1 text-foreground/75 hover:text-foreground"
        >
          <span className="truncate">{t("imageSource")}</span>
          <ExternalLink className="size-3 shrink-0" aria-hidden="true" />
        </button>
      </span>

      <ImagePreviewDialog
        src={src}
        alt={accessibleName}
        open={previewOpen && !failed}
        onOpenChange={setPreviewOpen}
      />
    </span>
  )
}
