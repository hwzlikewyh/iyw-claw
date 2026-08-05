"use client"

import { AlertCircle, ExternalLink, File, FolderSearch } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { usePlatform } from "@/hooks/use-platform"
import type { TaskArtifactInfo } from "@/lib/api"
import {
  isLocalDesktop,
  openPath,
  openPathWithPicker,
  revealItemInDir,
} from "@/lib/platform"
import { copyTextFromMenu } from "@/lib/utils"

interface TaskArtifactFileRowProps {
  item: TaskArtifactInfo
  onView: (item: TaskArtifactInfo) => void
}

export function TaskArtifactFileRow({
  item,
  onView,
}: TaskArtifactFileRowProps) {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <button
          type="button"
          onClick={() => onView(item)}
          className="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-sidebar-accent"
        >
          <File className="size-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm">{item.displayName}</span>
            <span
              className="block truncate text-xs text-muted-foreground"
              title={item.path}
            >
              {item.path}
            </span>
          </span>
          {item.status !== "available" && (
            <AlertCircle className="size-3.5 shrink-0 text-destructive" />
          )}
        </button>
      </ContextMenuTrigger>
      <ArtifactContextMenu item={item} onView={onView} />
    </ContextMenu>
  )
}

function ArtifactContextMenu({ item, onView }: TaskArtifactFileRowProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const { isWindows } = usePlatform()
  const canUseSystem = isLocalDesktop() && item.status === "available"

  const runFileAction = async (action: () => Promise<void>) => {
    try {
      await action()
    } catch {
      toast.error(t("actionFailed"))
    }
  }

  const copyPath = async () => {
    if (await copyTextFromMenu(item.path)) toast.success(t("pathCopied"))
    else toast.error(t("actionFailed"))
  }

  return (
    <ContextMenuContent>
      <ContextMenuItem onSelect={() => onView(item)}>
        {t("view")}
      </ContextMenuItem>
      <ContextMenuItem onSelect={() => void copyPath()}>
        {t("copyPath")}
      </ContextMenuItem>
      {canUseSystem && (
        <ContextMenuItem
          onSelect={() => void runFileAction(() => openPath(item.path))}
        >
          <ExternalLink className="size-4" />
          {t("openDefault")}
        </ContextMenuItem>
      )}
      {canUseSystem && isWindows && (
        <ContextMenuItem
          onSelect={() =>
            void runFileAction(() => openPathWithPicker(item.path))
          }
        >
          {t("openWith")}
        </ContextMenuItem>
      )}
      {canUseSystem && (
        <ContextMenuItem
          onSelect={() => void runFileAction(() => revealItemInDir(item.path))}
        >
          <FolderSearch className="size-4" />
          {t("reveal")}
        </ContextMenuItem>
      )}
    </ContextMenuContent>
  )
}
