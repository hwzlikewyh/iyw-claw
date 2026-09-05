"use client"

import { TaskArtifactDialog } from "@/components/layout/task-artifact-dialog"
import { ImagePreviewDialog } from "@/components/ui/image-preview-dialog"
import type { TaskArtifactInfo } from "@/lib/api"

import { CurrentReplyArtifactsDialog } from "./current-reply-artifacts-dialog"

export function CurrentReplyArtifactDialogs({
  items,
  selected,
  selectedImage,
  imageItems,
  imageIndex,
  allOpen,
  onSelectArtifact,
  onSelectImage,
  onOpenAllChange,
}: {
  items: TaskArtifactInfo[]
  selected: TaskArtifactInfo | null
  selectedImage: TaskArtifactInfo | null
  imageItems: TaskArtifactInfo[]
  imageIndex: number
  allOpen: boolean
  onSelectArtifact: (item: TaskArtifactInfo | null) => void
  onSelectImage: (id: number | null) => void
  onOpenAllChange: (open: boolean) => void
}) {
  return (
    <>
      <TaskArtifactDialog
        artifact={selected}
        open={selected !== null}
        onOpenChange={(open) => !open && onSelectArtifact(null)}
      />
      {allOpen && (
        <CurrentReplyArtifactsDialog
          items={items}
          open
          onOpenChange={onOpenAllChange}
          onSelectImage={(item) => {
            onOpenAllChange(false)
            onSelectImage(item.id)
          }}
        />
      )}
      <ImagePreviewDialog
        src={selectedImage?.path ?? ""}
        alt={selectedImage?.displayName ?? ""}
        open={selectedImage !== null}
        onOpenChange={(open) => !open && onSelectImage(null)}
        navigation={
          selectedImage && imageItems.length > 1
            ? {
                index: imageIndex,
                total: imageItems.length,
                onIndexChange: (index) =>
                  onSelectImage(imageItems[index]?.id ?? null),
              }
            : undefined
        }
      />
    </>
  )
}
