"use client"

import { useEffect, useMemo, useState } from "react"
import {
  Database,
  File,
  FileArchive,
  FileBraces,
  FileCode2,
  FileImage,
  FileMusic,
  FileSpreadsheet,
  FileText,
  FileVideo,
  Folder,
  Link2,
  Type,
} from "lucide-react"

import { useActiveFolder } from "@/contexts/active-folder-context"
import type { TaskArtifactInfo } from "@/lib/api"
import { readFileBase64, readWorkspaceFileBase64 } from "@/lib/api"
import { findOwningFolder } from "@/lib/file-open-target"
import { toImageDataUrl } from "@/components/message/workspace-file-preview"
import { toAbsoluteFilePath } from "@/lib/file-path-display"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { cn } from "@/lib/utils"
import {
  artifactVisualKind,
  type ArtifactVisualKind,
} from "@/components/layout/task-artifact-type"

const THUMBNAIL_MAX_BYTES = 4 * 1024 * 1024

type IconSize = "sm" | "md"

interface TaskArtifactTypeIconProps {
  item: TaskArtifactInfo
  size?: IconSize
  className?: string
}

interface LoadedThumbnail {
  key: string
  src: string
}

export function TaskArtifactTypeIcon({
  item,
  size = "sm",
  className,
}: TaskArtifactTypeIconProps) {
  const kind = useMemo(() => artifactVisualKind(item), [item])
  const thumbnail = useArtifactThumbnail(item, kind)
  const boxClass = size === "md" ? "size-9 rounded-lg" : "size-8 rounded-md"
  const iconClass = size === "md" ? "size-5" : "size-4"

  if (kind === "image" && thumbnail) {
    return (
      <span
        aria-hidden="true"
        className={cn(
          "relative shrink-0 overflow-hidden bg-cover bg-center",
          boxClass,
          className
        )}
        style={{ backgroundImage: `url(${thumbnail})` }}
      />
    )
  }

  return (
    <span
      className={cn(
        "grid shrink-0 place-items-center text-white shadow-[inset_0_0_0_1px_rgb(255_255_255/20%)]",
        boxClass,
        colorClassForKind(kind),
        className
      )}
      aria-hidden="true"
    >
      {renderIcon(kind, iconClass)}
    </span>
  )
}

function renderIcon(kind: ArtifactVisualKind, className: string) {
  switch (kind) {
    case "folder":
      return <Folder className={className} strokeWidth={1.8} />
    case "link":
      return <Link2 className={className} strokeWidth={1.8} />
    case "image":
      return <FileImage className={className} strokeWidth={1.8} />
    case "video":
      return <FileVideo className={className} strokeWidth={1.8} />
    case "audio":
      return <FileMusic className={className} strokeWidth={1.8} />
    case "code":
      return <FileCode2 className={className} strokeWidth={1.8} />
    case "data":
      return <FileBraces className={className} strokeWidth={1.8} />
    case "document":
      return <FileText className={className} strokeWidth={1.8} />
    case "spreadsheet":
      return <FileSpreadsheet className={className} strokeWidth={1.8} />
    case "archive":
      return <FileArchive className={className} strokeWidth={1.8} />
    case "font":
      return <Type className={className} strokeWidth={1.8} />
    case "database":
      return <Database className={className} strokeWidth={1.8} />
    default:
      return <File className={className} strokeWidth={1.8} />
  }
}

function colorClassForKind(kind: ArtifactVisualKind): string {
  switch (kind) {
    case "folder":
      return "bg-amber-500"
    case "link":
      return "bg-blue-500"
    case "image":
      return "bg-sky-500"
    case "video":
      return "bg-rose-500"
    case "audio":
      return "bg-fuchsia-500"
    case "code":
      return "bg-indigo-500"
    case "data":
      return "bg-cyan-500"
    case "document":
      return "bg-violet-500"
    case "spreadsheet":
      return "bg-emerald-500"
    case "archive":
      return "bg-orange-500"
    case "font":
      return "bg-teal-500"
    case "database":
      return "bg-sky-600"
    default:
      return "bg-slate-500"
  }
}

function useArtifactThumbnail(
  item: TaskArtifactInfo,
  kind: ArtifactVisualKind
): string | null {
  const folders = useAppWorkspaceStore((state) => state.folders)
  const { activeFolder } = useActiveFolder()
  const [loaded, setLoaded] = useState<LoadedThumbnail | null>(null)
  const artifactFolderPath =
    folders.find((folder) => folder.id === item.folderId)?.path ??
    activeFolder?.path
  const folderKey = folders
    .map((folder) => `${folder.id}:${folder.path}`)
    .join("|")
  const thumbnailKey = `${item.path}:${item.lastCheckedAt}:${artifactFolderPath ?? ""}:${folderKey}`

  useEffect(() => {
    if (kind !== "image" || item.status !== "available") return

    let cancelled = false
    void readArtifactImage(item.path, folders, artifactFolderPath)
      .then((base64) => {
        if (!cancelled) {
          setLoaded({
            key: thumbnailKey,
            src: toImageDataUrl(item.path, base64),
          })
        }
      })
      .catch(() => {})

    return () => {
      cancelled = true
    }
  }, [artifactFolderPath, folderKey, folders, item, kind, thumbnailKey])

  return loaded?.key === thumbnailKey ? loaded.src : null
}

async function readArtifactImage(
  path: string,
  folders: ReadonlyArray<{ id: number; path: string }>,
  activeFolderPath?: string
): Promise<string> {
  const absolutePath = toAbsoluteFilePath(path, activeFolderPath)
  if (!absolutePath) throw new Error("relative artifact path has no folder")

  const owner = findOwningFolder(absolutePath, folders)
  if (owner) {
    return readWorkspaceFileBase64(
      owner.rootPath,
      owner.relPath,
      THUMBNAIL_MAX_BYTES
    )
  }

  return readFileBase64(absolutePath, THUMBNAIL_MAX_BYTES)
}
