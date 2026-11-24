"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Maximize2 } from "lucide-react"
import type { ImagePreviewNavigation } from "@/components/image-editor/image-editor-model"
import { ImageSequenceControls } from "@/components/image-editor/image-sequence-controls"
import { useGalleryWheel } from "@/components/image-editor/use-gallery-wheel"
import {
  WorkspaceFilePreview,
  type PreviewState,
} from "@/components/message/workspace-file-preview"
import { Button } from "@/components/ui/button"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import type { TaskArtifactInfo } from "@/lib/api"
import { artifactVisualKind } from "./task-artifact-type"

export function artifactImageNavigation(options: {
  items?: TaskArtifactInfo[]
  selected: TaskArtifactInfo
  onSelect?: (item: TaskArtifactInfo) => void
}): ImagePreviewNavigation | undefined {
  const images =
    options.items?.filter(
      (item) =>
        item.status === "available" && artifactVisualKind(item) === "image"
    ) ?? []
  const index = images.findIndex((item) => item.id === options.selected.id)
  if (index < 0 || images.length < 2 || !options.onSelect) return undefined
  return {
    index,
    total: images.length,
    onIndexChange: (next) => {
      if (images[next]) options.onSelect?.(images[next])
    },
  }
}

export function ArtifactImagePreview({
  state,
  title,
  navigation,
}: {
  state: PreviewState
  title: string
  navigation?: ImagePreviewNavigation
}) {
  const [open, setOpen] = useState(false)
  const wheelRef = useGalleryWheel(navigation, { enabled: !open })
  const src =
    state.status === "image"
      ? state.content
      : state.status === "media" && state.mediaType === "image"
        ? state.src
        : ""
  return (
    <div ref={wheelRef} className="flex h-full min-h-0 flex-col">
      <ArtifactImageToolbar
        navigation={navigation}
        ready={Boolean(src)}
        onOpen={() => setOpen(true)}
      />
      <div className="min-h-0 flex-1">
        {src ? (
          <ArtifactImage
            key={src}
            src={src}
            title={title}
            onOpen={() => setOpen(true)}
          />
        ) : (
          <WorkspaceFilePreview state={state} rootPath="" displayName={title} />
        )}
      </div>
      <ImagePreviewDialog
        src={src}
        alt={title}
        open={open}
        loading={state.status === "loading"}
        navigation={navigation}
        onOpenChange={setOpen}
      />
    </div>
  )
}

function ArtifactImageToolbar({
  navigation,
  ready,
  onOpen,
}: {
  navigation?: ImagePreviewNavigation
  ready: boolean
  onOpen: () => void
}) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div className="flex shrink-0 items-center justify-end gap-2 border-b px-2 py-1">
      <ImageSequenceControls navigation={navigation} />
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        disabled={!ready}
        aria-label={t("imageViewerTitle")}
        title={t("imageViewerTitle")}
        onClick={onOpen}
      >
        <Maximize2 />
      </Button>
    </div>
  )
}

function ArtifactImage({
  src,
  title,
  onOpen,
}: {
  src: string
  title: string
  onOpen: () => void
}) {
  const [failed, setFailed] = useState(false)
  const t = useTranslations("Folder.chat.messageList")
  if (failed)
    return (
      <div
        role="status"
        className="flex h-full items-center justify-center text-sm text-muted-foreground"
      >
        {t("imageEditorLoadFailed")}
      </div>
    )
  return (
    <button
      type="button"
      onClick={onOpen}
      aria-label={t("imageViewerTitle")}
      className="flex size-full cursor-zoom-in items-center justify-center overflow-hidden bg-muted/20 p-6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
    >
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={src}
        alt={title}
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
        className="max-h-full max-w-full object-contain"
      />
    </button>
  )
}
