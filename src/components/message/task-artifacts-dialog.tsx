"use client"

import { useState } from "react"
import { PackageOpen } from "lucide-react"
import { useTranslations } from "next-intl"

import { CollapsedOverlayChip } from "@/components/chat/collapsed-overlay-chip"
import { TaskArtifactsTab } from "@/components/layout/aux-panel-artifacts-tab"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { useActiveFolder } from "@/contexts/active-folder-context"

interface TaskArtifactsDialogProps {
  conversationId: number
}

export function TaskArtifactsDialog({
  conversationId,
}: TaskArtifactsDialogProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const { activeFolder } = useActiveFolder()
  const [open, setOpen] = useState(false)

  if (!activeFolder) return null

  return (
    <>
      <CollapsedOverlayChip
        icon={<PackageOpen className="size-4 sm:size-[18px]" />}
        summary={t("open")}
        onClick={() => setOpen(true)}
      />
      <Dialog open={open} onOpenChange={setOpen}>
        {open && (
          <DialogContent className="flex h-[min(46rem,calc(100dvh-2rem))] max-w-[min(72rem,calc(100vw-2rem))] flex-col gap-0 overflow-hidden rounded-lg p-0 sm:max-w-[min(72rem,calc(100vw-2rem))]">
            <header className="flex h-11 shrink-0 items-center gap-2 border-b bg-muted/15 px-3 pr-12">
              <PackageOpen className="size-4 text-muted-foreground" />
              <DialogTitle className="text-sm font-medium">
                {t("title")}
              </DialogTitle>
              <DialogDescription className="sr-only">
                {t("description")}
              </DialogDescription>
            </header>
            <TaskArtifactsTab
              conversationId={conversationId}
              displayMode="browser"
              onRequestClose={() => setOpen(false)}
            />
          </DialogContent>
        )}
      </Dialog>
    </>
  )
}
