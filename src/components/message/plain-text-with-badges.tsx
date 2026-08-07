"use client"

import Image from "next/image"
import { memo, useEffect, useMemo, useState } from "react"
import { TriangleAlert } from "lucide-react"
import { useTranslations } from "next-intl"

import { ReferenceBadge } from "@/components/chat/composer/badges/reference-badge"
import { useOpenLinkOrFile } from "@/components/ai-elements/link-safety"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import { prepareChatImagePath, type PreparedChatImage } from "@/lib/api"
import { isDesktop } from "@/lib/platform"
import { fileUriToPath } from "@/lib/reference-link"
import { getActiveRemoteConnectionId } from "@/lib/transport"
import { cn } from "@/lib/utils"
import type { ReferenceAttrs } from "@/components/chat/composer/types"

import { parseUserMessageSegments } from "./user-message-segments"

const PREVIEWABLE_IMAGE_EXTENSIONS = new Set([
  "gif",
  "jpeg",
  "jpg",
  "png",
  "webp",
])

function isPreviewableImage(attrs: ReferenceAttrs): boolean {
  if (attrs.refType !== "file" || !attrs.uri) return false
  const plainLabel = attrs.label.replace(/:\d+(?:-\d+)?$/, "")
  const extension = plainLabel.split(".").pop()?.toLowerCase() ?? ""
  return PREVIEWABLE_IMAGE_EXTENSIONS.has(extension)
}

function FileReference({
  attrs,
  statusLabel,
}: {
  attrs: ReferenceAttrs
  statusLabel?: string
}) {
  const t = useTranslations("Folder.chat.messageList")
  const openTarget = useOpenLinkOrFile()
  return (
    <button
      type="button"
      title={statusLabel ?? attrs.label}
      aria-label={
        statusLabel ?? t("fileAttachmentLabel", { name: attrs.label })
      }
      onClick={() => void openTarget(attrs.uri!)}
      className="inline-flex max-w-full cursor-pointer appearance-none align-middle leading-none hover:opacity-80"
    >
      <ReferenceBadge data={attrs} />
      {statusLabel && (
        <TriangleAlert
          className="ms-1 h-3.5 w-3.5 shrink-0 self-center text-amber-600"
          aria-hidden
        />
      )}
    </button>
  )
}

function HistoricalImageReference({ attrs }: { attrs: ReferenceAttrs }) {
  const t = useTranslations("Folder.chat.messageList")
  const [image, setImage] = useState<PreparedChatImage | null>(null)
  const [previewFailed, setPreviewFailed] = useState(false)
  const [previewOpen, setPreviewOpen] = useState(false)
  useEffect(() => {
    const path = attrs.uri ? fileUriToPath(attrs.uri) : null
    if (!path) return
    let cancelled = false
    const source =
      isDesktop() && getActiveRemoteConnectionId() === null
        ? "local"
        : "workspace"
    void prepareChatImagePath(path, source)
      .then((prepared) => {
        if (!cancelled) {
          setImage(prepared)
          setPreviewFailed(false)
        }
      })
      .catch(() => {
        if (!cancelled) setPreviewFailed(true)
      })
    return () => {
      cancelled = true
    }
  }, [attrs.uri])
  if (!image) {
    return (
      <FileReference
        attrs={attrs}
        statusLabel={
          previewFailed
            ? t("imagePreviewUnavailable", { name: attrs.label })
            : undefined
        }
      />
    )
  }
  const src = `data:${image.mimeType};base64,${image.data}`
  const imageLabel = t("imageAttachmentLabel", { name: attrs.label })
  return (
    <span className="inline-flex align-middle">
      <button
        type="button"
        title={imageLabel}
        aria-label={imageLabel}
        onClick={() => setPreviewOpen(true)}
        className="overflow-hidden rounded-md border border-border/70 bg-muted/30 transition-opacity hover:opacity-80"
      >
        <Image
          src={src}
          alt={imageLabel}
          width={56}
          height={56}
          unoptimized
          className="h-14 w-14 object-cover"
        />
      </button>
      <ImagePreviewDialog
        src={src}
        alt={imageLabel}
        open={previewOpen}
        onOpenChange={setPreviewOpen}
      />
    </span>
  )
}

export const PlainTextWithBadges = memo(function PlainTextWithBadges({
  text,
  className,
}: {
  text: string
  className?: string
}) {
  const segments = useMemo(() => parseUserMessageSegments(text), [text])
  return (
    <div className={cn("whitespace-pre-wrap break-words", className)}>
      {segments.map((segment, index) =>
        segment.kind === "reference" ? (
          segment.attrs.refType === "file" && segment.attrs.uri ? (
            isPreviewableImage(segment.attrs) ? (
              <HistoricalImageReference key={index} attrs={segment.attrs} />
            ) : (
              <FileReference key={index} attrs={segment.attrs} />
            )
          ) : (
            <ReferenceBadge key={index} data={segment.attrs} />
          )
        ) : (
          <span key={index}>{segment.text}</span>
        )
      )}
    </div>
  )
})
