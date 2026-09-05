"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"

import { TaskArtifactFileRow } from "@/components/layout/task-artifact-file-row"
import { TaskArtifactPreview } from "@/components/layout/task-artifact-preview"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import type { TaskArtifactInfo } from "@/lib/api"
import { cn } from "@/lib/utils"

import { isRemoteImageArtifact } from "./current-reply-image-artifacts"

export function CurrentReplyArtifactsDialog({
  items,
  open,
  onOpenChange,
  onSelectImage,
}: {
  items: TaskArtifactInfo[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onSelectImage?: (item: TaskArtifactInfo) => void
}) {
  const [selected, setSelected] = useState<TaskArtifactInfo | null>(
    items[0] ?? null
  )
  const [mobilePreviewOpen, setMobilePreviewOpen] = useState(false)
  const t = useTranslations("Folder.taskArtifacts")

  const select = (item: TaskArtifactInfo) => {
    const scopedItem = items.find((candidate) => candidate.id === item.id)
    if (!scopedItem) return
    if (isRemoteImageArtifact(scopedItem) && onSelectImage) {
      onSelectImage(scopedItem)
      return
    }
    setSelected(scopedItem)
    setMobilePreviewOpen(true)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="h-[min(42rem,calc(100dvh-2rem))] max-w-[min(72rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0 sm:max-w-[min(72rem,calc(100vw-2rem))]">
        <DialogTitle className="sr-only">{t("currentReplyTitle")}</DialogTitle>
        <DialogDescription className="sr-only">
          {t("currentReplyCount", { count: items.length })}
        </DialogDescription>
        <ArtifactsDialogBody
          items={items}
          selected={selected}
          mobilePreviewOpen={mobilePreviewOpen}
          onSelect={select}
          onBack={() => setMobilePreviewOpen(false)}
          onOpenWorkspace={() => onOpenChange(false)}
        />
      </DialogContent>
    </Dialog>
  )
}

function ArtifactsDialogBody({
  items,
  selected,
  mobilePreviewOpen,
  onSelect,
  onBack,
  onOpenWorkspace,
}: {
  items: TaskArtifactInfo[]
  selected: TaskArtifactInfo | null
  mobilePreviewOpen: boolean
  onSelect: (item: TaskArtifactInfo) => void
  onBack: () => void
  onOpenWorkspace: () => void
}) {
  return (
    <div className="grid min-h-0 grid-rows-[minmax(0,1fr)] md:grid-cols-[minmax(15rem,18rem)_minmax(0,1fr)]">
      <ArtifactList
        items={items}
        selected={selected}
        hidden={mobilePreviewOpen}
        onSelect={onSelect}
        onOpenWorkspace={onOpenWorkspace}
      />
      <TaskArtifactPreview
        artifact={selected}
        className={cn("h-full", mobilePreviewOpen ? "grid" : "hidden md:grid")}
        onBack={onBack}
        onOpenWorkspace={onOpenWorkspace}
        onPreview={onSelect}
      />
    </div>
  )
}

function ArtifactList({
  items,
  selected,
  hidden,
  onSelect,
  onOpenWorkspace,
}: {
  items: TaskArtifactInfo[]
  selected: TaskArtifactInfo | null
  hidden: boolean
  onSelect: (item: TaskArtifactInfo) => void
  onOpenWorkspace: () => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <section
      aria-label={t("currentReplyTitle")}
      className={cn(
        "min-h-0 flex-col border-r bg-card/30",
        hidden ? "hidden md:flex" : "flex"
      )}
    >
      <div className="border-b px-3 py-3 pr-12">
        <div className="text-sm font-medium">{t("currentReplyTitle")}</div>
        <div className="text-xs text-muted-foreground">
          {t("currentReplyCount", { count: items.length })}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-1">
        {items.map((item) => (
          <TaskArtifactFileRow
            key={item.id}
            item={item}
            selected={selected?.id === item.id}
            openOnDoubleClick
            onSelect={onSelect}
            onOpenWorkspace={onOpenWorkspace}
          />
        ))}
      </div>
    </section>
  )
}
