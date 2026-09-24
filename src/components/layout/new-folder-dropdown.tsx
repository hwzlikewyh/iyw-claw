"use client"

import { useRef, useState } from "react"
import { FolderPlus } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { isDesktop, openFileDialog } from "@/lib/platform"
import { getActiveRemoteConnectionId, getTransport } from "@/lib/transport"
import type { FolderDetail } from "@/lib/types"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { DirectoryBrowserDialog } from "@/components/shared/directory-browser-dialog"
import { cn } from "@/lib/utils"
import { toast } from "sonner"
import { toErrorMessage } from "@/lib/app-error"

export function NewFolderDropdown({
  showLabel = false,
  buttonClassName,
  label,
  onOpened,
}: {
  showLabel?: boolean
  buttonClassName?: string
  label?: string
  onOpened?: (folder: FolderDetail) => void
}) {
  const t = useTranslations("Folder.folderNameDropdown")
  const openFolder = useAppWorkspaceStore((s) => s.openFolder)
  const [browserOpen, setBrowserOpen] = useState(false)
  const [pending, setPending] = useState(false)
  const busy = useRef(false)
  const picking = useRef(false)
  const td = useTranslations("SidebarDesign")
  async function selectFolder(path: string) {
    if (busy.current) return
    busy.current = true
    setPending(true)
    const backend = getTransport()
    try {
      const folder = await openFolder(path)
      if (backend === getTransport()) onOpened?.(folder)
    } catch (error) {
      console.error("[sidebar-project] open folder failed", {
        message: toErrorMessage(error),
      })
      toast.error(td("folderFailed", { message: toErrorMessage(error) }))
    } finally {
      busy.current = false
      setPending(false)
    }
  }

  async function handleOpenFolder() {
    if (busy.current || picking.current) return
    picking.current = true
    // Only use the native Tauri directory dialog when running on the local
    // desktop. In a remote workspace window we're still inside Tauri, but the
    // folder we want lives on the remote host — the native dialog would
    // browse the *local* filesystem and produce a path the remote server
    // can't open. Fall through to the in-app server-side browser instead.
    if (isDesktop() && getActiveRemoteConnectionId() === null) {
      try {
        setPending(true)
        const selected = await openFileDialog({
          directory: true,
          multiple: false,
        })
        if (selected && getActiveRemoteConnectionId() === null) {
          const path = Array.isArray(selected) ? selected[0] : selected
          if (path) await selectFolder(path)
        }
      } catch (error) {
        toast.error(td("folderFailed", { message: toErrorMessage(error) }))
      } finally {
        picking.current = false
        setPending(false)
      }
    } else {
      setBrowserOpen(true)
    }
  }

  return (
    <>
      <Button
        variant="ghost"
        size="icon"
        className={cn(
          showLabel
            ? "h-10 w-full justify-start gap-2 px-3 text-[0.875rem]"
            : "h-6 w-6",
          "hover:text-foreground/80",
          buttonClassName
        )}
        title={label || t("openFolder")}
        aria-label={label || t("openFolder")}
        disabled={pending}
        onClick={handleOpenFolder}
      >
        <FolderPlus className="h-3.5 w-3.5" />
        {showLabel ? (
          <span className="truncate">{label || t("openFolder")}</span>
        ) : null}
      </Button>
      <DirectoryBrowserDialog
        open={browserOpen}
        onOpenChange={(open) => {
          setBrowserOpen(open)
          if (!open) picking.current = false
        }}
        onSelect={(path) => {
          void selectFolder(path)
        }}
      />
    </>
  )
}
