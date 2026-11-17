"use client"

import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { DialogFooter } from "@/components/ui/dialog"
import type { ProjectReferenceSelection } from "./project-reference-artifacts"

interface ProjectReferenceFooterProps {
  selected: ProjectReferenceSelection | null
  onCancel: () => void
  onConfirm: () => void
}

export function ProjectReferenceFooter({
  selected,
  onCancel,
  onConfirm,
}: ProjectReferenceFooterProps) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const common = useTranslations("Folder.common")
  return (
    <DialogFooter className="items-center sm:justify-between">
      <span
        className="min-w-0 flex-1 truncate text-xs text-muted-foreground"
        title={selected?.path}
      >
        {selected?.name ?? t("nothingSelected")}
      </span>
      <div className="flex min-w-0 items-center gap-2">
        <Button variant="outline" onClick={onCancel}>
          {common("cancel")}
        </Button>
        <Button disabled={!selected} onClick={onConfirm}>
          {t("confirm")}
        </Button>
      </div>
    </DialogFooter>
  )
}
