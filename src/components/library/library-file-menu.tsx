"use client"

import {
  Download,
  Eye,
  FileCog,
  FolderInput,
  Info,
  MoreHorizontal,
  Pencil,
  RefreshCw,
  Scissors,
  Trash2,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { LibraryFile } from "@/lib/knowledge-library"

export type LibraryFileAction =
  | "view"
  | "design"
  | "details"
  | "rename"
  | "move"
  | "refresh"
  | "download"
  | "delete"
  | "points"

export function LibraryFileMenu({
  file,
  onAction,
}: {
  file: LibraryFile
  onAction: (action: LibraryFileAction, file: LibraryFile) => void
}) {
  const t = useTranslations("Library")
  const item = (
    action: LibraryFileAction,
    icon: React.ReactNode,
    label: string,
    destructive = false
  ) => (
    <DropdownMenuItem
      key={action}
      variant={destructive ? "destructive" : "default"}
      onSelect={() => onAction(action, file)}
    >
      {icon}
      {label}
    </DropdownMenuItem>
  )
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("moreActions")}
          onClick={(event) => event.stopPropagation()}
        >
          <MoreHorizontal className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-44">
        {item("view", <Eye />, t("viewDocument"))}
        {item("design", <FileCog />, t("designParse"))}
        {item("details", <Info />, t("details"))}
        {item("points", <Scissors />, t("sliceManagement"))}
        <DropdownMenuSeparator />
        {item("rename", <Pencil />, t("rename"))}
        {item("move", <FolderInput />, t("moveTo"))}
        {item("refresh", <RefreshCw />, t("refreshStatus"))}
        {item("download", <Download />, t("download"))}
        {item("delete", <Trash2 />, t("delete"), true)}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
