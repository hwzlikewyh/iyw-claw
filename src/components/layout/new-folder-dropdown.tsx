"use client"

import { useState } from "react"
import { FolderPlus } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { isDesktop, openFileDialog } from "@/lib/platform"
import { getActiveRemoteConnectionId } from "@/lib/transport"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { DirectoryBrowserDialog } from "@/components/shared/directory-browser-dialog"

export function NewFolderDropdown() {
  const t = useTranslations("Folder.folderNameDropdown")
  const openFolder = useAppWorkspaceStore((s) => s.openFolder)
  const [browserOpen, setBrowserOpen] = useState(false)

  async function handleOpenFolder() {
    // Only use the native Tauri directory dialog when running on the local
    // desktop. In a remote workspace window we're still inside Tauri, but the
    // folder we want lives on the remote host — the native dialog would
    // browse the *local* filesystem and produce a path the remote server
    // can't open. Fall through to the in-app server-side browser instead.
    if (isDesktop() && getActiveRemoteConnectionId() === null) {
      const selected = await openFileDialog({
        directory: true,
        multiple: false,
      })
      if (selected) {
        await openFolder(Array.isArray(selected) ? selected[0] : selected)
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
        className="h-6 w-6 hover:text-foreground/80"
        title={t("openFolder")}
        aria-label={t("openFolder")}
        onClick={handleOpenFolder}
      >
        <FolderPlus className="h-3.5 w-3.5" />
      </Button>
      <DirectoryBrowserDialog
        open={browserOpen}
        onOpenChange={setBrowserOpen}
        onSelect={(path) => {
          openFolder(path).catch((err) => {
            console.error("[NewFolderDropdown] failed to open folder:", err)
          })
        }}
      />
    </>
  )
}
