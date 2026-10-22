"use client"

import { useState } from "react"
import { ChevronRight, Folder, FolderOpen } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useTranslations } from "next-intl"
import type { KnowledgeFolder } from "@/lib/cloud-knowledge"
import { cn } from "@/lib/utils"

interface FolderTreeProps {
  folders: KnowledgeFolder[]
  selected: string
  onSelect: (id: string) => void
}

export function LibraryFolders(props: FolderTreeProps) {
  const t = useTranslations("Library")
  return (
    <nav aria-label={t("folders")} className="min-h-0 overflow-y-auto p-2">
      <h2 className="px-2 py-3 text-xs font-medium text-muted-foreground">
        {t("folders")}
      </h2>
      {props.folders.length ? (
        props.folders.map((folder) => (
          <FolderRow key={folder.folderId} folder={folder} tree={props} />
        ))
      ) : (
        <p className="p-3 text-sm text-muted-foreground">{t("emptyLibrary")}</p>
      )}
    </nav>
  )
}

function FolderRow({
  folder,
  tree,
}: {
  folder: KnowledgeFolder
  tree: FolderTreeProps
}) {
  const [expanded, setExpanded] = useState(true)
  const active = tree.selected === folder.folderId
  const t = useTranslations("Library")
  return (
    <div>
      <div
        className={cn(
          "flex min-w-0 items-center rounded-md",
          active && "bg-muted"
        )}
      >
        {folder.children.length > 0 ? (
          <button
            type="button"
            onClick={() => setExpanded(!expanded)}
            aria-expanded={expanded}
            aria-label={t(expanded ? "collapseFolder" : "expandFolder", {
              name: folder.folderName,
            })}
            className="flex size-7 shrink-0 items-center justify-center rounded-sm hover:bg-muted focus-visible:outline focus-visible:outline-ring"
          >
            <ChevronRight className={cn("size-3", expanded && "rotate-90")} />
          </button>
        ) : (
          <span className="w-7 shrink-0" />
        )}
        <FolderName folder={folder} tree={tree} />
      </div>
      {expanded && folder.children.length > 0 && (
        <div className="pl-3">
          {folder.children.map((child) => (
            <FolderRow key={child.folderId} folder={child} tree={tree} />
          ))}
        </div>
      )}
    </div>
  )
}

function FolderName({
  folder,
  tree,
}: {
  folder: KnowledgeFolder
  tree: FolderTreeProps
}) {
  const active = tree.selected === folder.folderId
  const Icon = active ? FolderOpen : Folder
  return (
    <button
      type="button"
      onClick={() => tree.onSelect(folder.folderId)}
      aria-current={active ? "location" : undefined}
      title={folder.folderName}
      className="flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-md pr-2 text-left text-sm hover:bg-muted focus-visible:outline focus-visible:outline-ring"
    >
      <Icon
        className="size-4 shrink-0 text-muted-foreground"
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 truncate">{folder.folderName}</span>
      <span className="text-xs tabular-nums text-muted-foreground">
        {folder.fileCount}
      </span>
    </button>
  )
}

export function MobileFolderPicker(props: FolderTreeProps) {
  const [open, setOpen] = useState(false)
  const t = useTranslations("Library")
  return (
    <div className="mb-4 md:hidden">
      <Button
        variant="outline"
        className="min-h-11 w-full justify-start"
        onClick={() => setOpen(true)}
      >
        <FolderOpen className="size-4" />
        {t("chooseFolder")}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent
          className="max-h-[80dvh] rounded-lg p-4"
          aria-describedby={undefined}
        >
          <DialogHeader>
            <DialogTitle>{t("chooseFolder")}</DialogTitle>
          </DialogHeader>
          <LibraryFolders
            {...props}
            onSelect={(id) => {
              props.onSelect(id)
              setOpen(false)
            }}
          />
        </DialogContent>
      </Dialog>
    </div>
  )
}

export function FolderBreadcrumb({
  path,
  onSelect,
}: {
  path: KnowledgeFolder[]
  onSelect: (id: string) => void
}) {
  const t = useTranslations("Library")
  return (
    <nav
      aria-label={t("currentFolder")}
      className="flex min-w-0 flex-wrap items-center gap-1 text-sm"
    >
      {path.map((folder, index) => (
        <span
          key={folder.folderId}
          className="inline-flex min-w-0 max-w-full items-center gap-1"
        >
          {index > 0 && (
            <ChevronRight
              className="size-3 shrink-0 text-muted-foreground"
              aria-hidden="true"
            />
          )}
          <button
            type="button"
            title={folder.folderName}
            onClick={() => onSelect(folder.folderId)}
            aria-current={index === path.length - 1 ? "location" : undefined}
            className="max-w-52 truncate rounded px-1 py-1 text-left hover:bg-muted focus-visible:outline focus-visible:outline-ring"
          >
            {folder.folderName}
          </button>
        </span>
      ))}
    </nav>
  )
}
