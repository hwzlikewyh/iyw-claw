"use client"

import { useEffect, useMemo, useState } from "react"
import {
  ExternalLink,
  FolderSearch,
  MoreHorizontal,
  PanelsTopLeft,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  WorkspaceFilePreview,
  type PreviewState,
} from "@/components/message/workspace-file-preview"
import { loadWorkspacePreview } from "@/components/message/workspace-file-preview-loader"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import type { TaskArtifactInfo } from "@/lib/api"
import { findOwningFolder } from "@/lib/file-open-target"
import { isOfficePreviewable } from "@/lib/language-detect"
import {
  isLocalDesktop,
  openPath,
  openPathWithPicker,
  revealItemInDir,
} from "@/lib/platform"
import { copyTextToClipboard } from "@/lib/utils"
import { usePlatform } from "@/hooks/use-platform"

interface TaskArtifactDialogProps {
  artifact: TaskArtifactInfo | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface LoadedPreview {
  key: string
  state: PreviewState
}

export function TaskArtifactDialog({
  artifact,
  open,
  onOpenChange,
}: TaskArtifactDialogProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const { isWindows } = usePlatform()
  const { activeFolder } = useActiveFolder()
  const { openFilePreview } = useWorkspaceActions()
  const [loadedPreview, setLoadedPreview] = useState<LoadedPreview | null>(null)
  const target = useMemo(() => {
    if (!artifact || !activeFolder || artifact.folderId !== activeFolder.id) {
      return null
    }
    const owning = findOwningFolder(artifact.path, [activeFolder])
    return owning ? { rootPath: owning.rootPath, ioPath: owning.relPath } : null
  }, [activeFolder, artifact])
  const isAvailable = artifact?.status === "available"
  const canUseSystem = isLocalDesktop() && isAvailable
  const canOpenWorkspace = target !== null && isAvailable
  const previewKey =
    open && artifact && target && artifact.status === "available"
      ? `${artifact.id}:${artifact.lastCheckedAt}`
      : null

  const preview = useMemo<PreviewState>(() => {
    if (!open || !artifact) return { status: "idle" }
    if (artifact.status !== "available") {
      return {
        status: "error",
        path: artifact.path,
        message: t("fileUnavailable"),
      }
    }
    if (!target) {
      return {
        status: "error",
        path: artifact.path,
        message: t("previewOutsideWorkspace"),
      }
    }
    if (isOfficePreviewable(target.ioPath)) {
      return { status: "office", path: target.ioPath }
    }
    if (previewKey && loadedPreview?.key === previewKey) {
      return loadedPreview.state
    }
    return { status: "loading", path: target.ioPath }
  }, [artifact, loadedPreview, open, previewKey, t, target])

  useEffect(() => {
    if (!previewKey || !target || isOfficePreviewable(target.ioPath)) return
    let active = true
    void loadWorkspacePreview(target.rootPath, target.ioPath)
      .then(
        (next) => active && setLoadedPreview({ key: previewKey, state: next })
      )
      .catch(
        () =>
          active &&
          setLoadedPreview({
            key: previewKey,
            state: {
              status: "error",
              path: target.ioPath,
              message: t("previewFailed"),
            },
          })
      )
    return () => {
      active = false
    }
  }, [previewKey, t, target])

  if (!artifact) return null

  const runAction = async (action: () => Promise<void>, success?: string) => {
    try {
      await action()
      if (success) toast.success(success)
    } catch {
      toast.error(t("actionFailed"))
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="grid h-[min(42rem,calc(100dvh-2rem))] max-w-[min(64rem,calc(100vw-2rem))] grid-rows-[auto_minmax(0,1fr)] gap-0 overflow-hidden p-0 sm:max-w-[min(64rem,calc(100vw-2rem))]">
        <DialogTitle className="sr-only">{artifact.displayName}</DialogTitle>
        <DialogDescription className="sr-only">
          {artifact.path}
        </DialogDescription>
        <header className="flex h-12 min-w-0 items-center gap-2 border-b px-3 pr-12">
          <PanelsTopLeft className="size-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium">
              {artifact.displayName}
            </p>
            <p
              className="truncate text-xs text-muted-foreground"
              title={artifact.path}
            >
              {artifact.path}
            </p>
          </div>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={t("moreActions")}
                title={t("moreActions")}
              >
                <MoreHorizontal className="size-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem
                onSelect={() =>
                  void runAction(async () => {
                    if (!(await copyTextToClipboard(artifact.path)))
                      throw new Error()
                  }, t("pathCopied"))
                }
              >
                {t("copyPath")}
              </DropdownMenuItem>
              {canUseSystem && (
                <DropdownMenuItem
                  onSelect={() => void runAction(() => openPath(artifact.path))}
                >
                  <ExternalLink className="size-4" />
                  {t("openDefault")}
                </DropdownMenuItem>
              )}
              {canUseSystem && isWindows && (
                <DropdownMenuItem
                  onSelect={() =>
                    void runAction(() => openPathWithPicker(artifact.path))
                  }
                >
                  {t("openWith")}
                </DropdownMenuItem>
              )}
              {canUseSystem && (
                <DropdownMenuItem
                  onSelect={() =>
                    void runAction(() => revealItemInDir(artifact.path))
                  }
                >
                  <FolderSearch className="size-4" />
                  {t("reveal")}
                </DropdownMenuItem>
              )}
              {canOpenWorkspace && (
                <DropdownMenuItem
                  onSelect={() => void openFilePreview(artifact.path)}
                >
                  {t("openWorkspace")}
                </DropdownMenuItem>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </header>
        <div className="min-h-0">
          <WorkspaceFilePreview
            state={preview}
            rootPath={target?.rootPath ?? ""}
          />
        </div>
      </DialogContent>
    </Dialog>
  )
}
