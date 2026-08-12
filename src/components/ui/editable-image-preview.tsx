"use client"

import { useState, type ComponentProps } from "react"
import { Pencil } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import { cn } from "@/lib/utils"

type ImageProps = Omit<ComponentProps<"img">, "alt" | "src">

export interface EditableImagePreviewProps {
  src: string
  alt: string
  trigger: "edit-button" | "image"
  className?: string
  imageProps?: ImageProps
}

export function EditableImagePreview({
  src,
  alt,
  trigger,
  className,
  imageProps,
}: EditableImagePreviewProps) {
  const t = useTranslations("Folder.chat.messageList")
  const [loadedSource, setLoadedSource] = useState<string | null>(null)
  const [previewOpen, setPreviewOpen] = useState(false)
  const loaded = loadedSource === src
  return (
    <span className={cn("relative", className)}>
      <EditTrigger
        trigger={trigger}
        loaded={loaded}
        label={t("imageEditorTitle")}
        onOpen={() => setPreviewOpen(true)}
      >
        <PreviewImage
          src={src}
          alt={alt}
          imageProps={imageProps}
          onLoadedChange={setLoadedSource}
        />
      </EditTrigger>
      <ImagePreviewDialog
        src={src}
        alt={alt}
        open={previewOpen && loaded}
        onOpenChange={setPreviewOpen}
      />
    </span>
  )
}

function PreviewImage({
  src,
  alt,
  imageProps,
  onLoadedChange,
}: Pick<EditableImagePreviewProps, "src" | "alt" | "imageProps"> & {
  onLoadedChange: (source: string | null) => void
}) {
  return (
    // Remote Markdown image hosts are dynamic, so next/image cannot declare them.
    // eslint-disable-next-line @next/next/no-img-element
    <img
      {...imageProps}
      src={src}
      alt={alt}
      onLoad={(event) => {
        onLoadedChange(src)
        imageProps?.onLoad?.(event)
      }}
      onError={(event) => {
        onLoadedChange(null)
        imageProps?.onError?.(event)
      }}
    />
  )
}

function EditTrigger({
  trigger,
  loaded,
  label,
  onOpen,
  children,
}: Pick<EditableImagePreviewProps, "trigger"> & {
  loaded: boolean
  label: string
  onOpen: () => void
  children: React.ReactNode
}) {
  if (trigger === "image") {
    return (
      <button
        type="button"
        disabled={!loaded}
        onClick={onOpen}
        className="block max-w-full cursor-zoom-in appearance-none disabled:cursor-default"
        aria-label={label}
        title={label}
      >
        {children}
      </button>
    )
  }
  return (
    <>
      {children}
      <Button
        type="button"
        variant="secondary"
        size="icon-sm"
        disabled={!loaded}
        onClick={onOpen}
        className="absolute right-3 top-3 shadow-sm"
        aria-label={label}
        title={label}
      >
        <Pencil className="size-4" />
      </Button>
    </>
  )
}
