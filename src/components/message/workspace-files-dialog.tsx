"use client"

import { useState } from "react"
import { FolderTree } from "lucide-react"
import { useTranslations } from "next-intl"

import { CollapsedOverlayChip } from "@/components/chat/collapsed-overlay-chip"
import { WorkspaceDirectoryBrowser } from "@/components/message/workspace-directory-browser"
import { prefetchWorkspaceRoot } from "@/components/message/workspace-file-tree-data"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { useActiveFolder } from "@/contexts/active-folder-context"

function WorkspaceFilesDialogContent({ rootPath }: { rootPath: string }) {
  const t = useTranslations("Folder.chat.workspaceFiles")

  return (
    <DialogContent
      closeButtonClassName="top-2 right-2 z-20 bg-background/70"
      className="h-[min(46rem,calc(100dvh-2rem))] max-w-[min(72rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-lg p-0 sm:max-w-[min(72rem,calc(100vw-2rem))]"
    >
      <DialogTitle className="sr-only">{t("title")}</DialogTitle>
      <DialogDescription className="sr-only">
        {t("description")}
      </DialogDescription>
      <WorkspaceDirectoryBrowser rootPath={rootPath} className="h-full" />
    </DialogContent>
  )
}

export function WorkspaceFilesDialog() {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const { activeFolder } = useActiveFolder()
  const [open, setOpen] = useState(false)
  const rootPath = activeFolder?.path ?? null
  if (!rootPath) return null

  return (
    <>
      <div
        onPointerEnter={() => prefetchWorkspaceRoot(rootPath)}
        onFocus={() => prefetchWorkspaceRoot(rootPath)}
      >
        <CollapsedOverlayChip
          icon={<FolderTree className="size-4 sm:size-[18px]" />}
          summary={t("open")}
          onClick={() => setOpen(true)}
        />
      </div>
      <Dialog open={open} onOpenChange={setOpen}>
        {open && (
          <WorkspaceFilesDialogContent key={rootPath} rootPath={rootPath} />
        )}
      </Dialog>
    </>
  )
}
