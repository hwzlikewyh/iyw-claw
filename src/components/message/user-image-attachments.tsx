"use client"

import { useCallback, useState } from "react"
import Image from "next/image"
import { Download } from "lucide-react"
import { useTranslations } from "next-intl"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import type { UserImageDisplay } from "@/lib/adapters/ai-elements-adapter"
import { toErrorMessage } from "@/lib/app-error"
import { downloadImage } from "@/lib/image-download"

interface UserImageAttachmentsProps {
  images: UserImageDisplay[]
  className?: string
}

interface ImageThumbnailProps {
  image: UserImageDisplay
  index: number
  downloadLabel: string
  onOpen: (index: number) => void
  onDownload: (image: UserImageDisplay) => void
}

function ImageThumbnail(props: ImageThumbnailProps) {
  return (
    <div className="group relative overflow-hidden rounded-md border border-border/70 bg-muted/30">
      <button
        type="button"
        onClick={() => props.onOpen(props.index)}
        className="block cursor-pointer transition-opacity hover:opacity-80"
      >
        <Image
          src={`data:${props.image.mime_type};base64,${props.image.data}`}
          alt={props.image.name}
          width={56}
          height={56}
          unoptimized
          className="h-14 w-14 object-cover"
        />
      </button>
      <button
        type="button"
        onClick={(event) => {
          event.stopPropagation()
          props.onDownload(props.image)
        }}
        className="absolute right-0.5 top-0.5 rounded-full bg-background/80 p-0.5 text-foreground/80 opacity-0 shadow-sm transition-opacity hover:bg-background hover:text-foreground group-hover:opacity-100 focus-visible:opacity-100"
        aria-label={props.downloadLabel}
        title={props.downloadLabel}
      >
        <Download className="h-3 w-3" />
      </button>
    </div>
  )
}

function AttachmentPreview({
  images,
  index,
  onIndexChange,
}: {
  images: UserImageDisplay[]
  index: number | null
  onIndexChange: (index: number | null) => void
}) {
  const image = index !== null && index < images.length ? images[index] : null
  return (
    <ImagePreviewDialog
      src={image ? `data:${image.mime_type};base64,${image.data}` : ""}
      alt={image?.name ?? ""}
      open={image !== null}
      onOpenChange={(open) => !open && onIndexChange(null)}
      navigation={
        index !== null
          ? { index, total: images.length, onIndexChange }
          : undefined
      }
    />
  )
}

export function UserImageAttachments({
  images,
  className,
}: UserImageAttachmentsProps) {
  const t = useTranslations("Folder.chat.messageList")
  const [previewIndex, setPreviewIndex] = useState<number | null>(null)
  const handleDownload = useCallback(
    async (image: UserImageDisplay) => {
      try {
        await downloadImage({
          data: image.data,
          mime_type: image.mime_type,
          suggestedName: image.name,
        })
      } catch (error) {
        window.alert(t("downloadFailed", { message: toErrorMessage(error) }))
      }
    },
    [t]
  )
  if (images.length === 0) return null
  return (
    <div className={className}>
      <div className="flex flex-wrap gap-1.5">
        {images.map((image, index) => (
          <ImageThumbnail
            key={`${image.uri ?? image.name}-${index}`}
            image={image}
            index={index}
            downloadLabel={t("downloadImage")}
            onOpen={setPreviewIndex}
            onDownload={(item) => void handleDownload(item)}
          />
        ))}
      </div>
      <AttachmentPreview
        images={images}
        index={previewIndex}
        onIndexChange={setPreviewIndex}
      />
    </div>
  )
}
