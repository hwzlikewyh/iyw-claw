"use client"

import { FolderSearch } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { DialogFooter } from "@/components/ui/dialog"
import type { ProjectReferenceSelection } from "./project-reference-artifacts"

interface ProjectReferenceFooterProps {
  selected: ProjectReferenceSelection | null
  onBrowseFolder: () => void
  onConfirm: () => void
}

export function ProjectReferenceFooter({
  selected,
  onBrowseFolder,
  onConfirm,
}: ProjectReferenceFooterProps) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  return (
    <DialogFooter className="items-center sm:justify-between">
      <Button variant="ghost" onClick={onBrowseFolder}>
        <FolderSearch className="size-4" />
        {t("browseFolder")}
      </Button>
      <div className="flex min-w-0 items-center gap-2">
        <span className="max-w-52 truncate text-xs text-muted-foreground">
          {selected?.name ?? t("nothingSelected")}
        </span>
        <Button disabled={!selected} onClick={onConfirm}>
          {t("confirm")}
        </Button>
      </div>
    </DialogFooter>
  )
}
