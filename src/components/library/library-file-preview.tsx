"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  WorkspaceFilePreview,
  type PreviewState,
} from "@/components/message/workspace-file-preview"
import { remoteArtifactFrameSrc } from "@/components/layout/task-artifact-preview-loader"
import { binaryPreviewKind } from "@/lib/binary-preview"
import { isImageFile } from "@/lib/language-detect"
import type { LibraryFile } from "@/lib/knowledge-library"
import { LibraryError } from "./library-states"

interface LibraryFilePreviewProps {
  file: LibraryFile
  onClose: () => void
}

export function LibraryFilePreview({ file, onClose }: LibraryFilePreviewProps) {
  const t = useTranslations("Library")
  const [attempt, setAttempt] = useState(0)
  const retry = () => setAttempt((value) => value + 1)
  const state = libraryPreviewState(file)
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="h-[min(48rem,calc(100dvh-2rem))] max-w-[min(72rem,calc(100vw-2rem))] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 overflow-hidden rounded-lg p-0">
        <DialogHeader className="border-b px-4 py-3">
          <DialogTitle>{t("viewDocument")}</DialogTitle>
          <DialogDescription className="truncate" title={file.fileName}>
            {file.fileName}
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 overflow-hidden">
          {state ? (
            <WorkspaceFilePreview
              key={attempt}
              state={state}
              displayName={file.fileName}
              rootPath=""
            />
          ) : (
            <LibraryError error={t("fileUrlUnavailable")} retry={retry} />
          )}
        </div>
        <DialogFooter className="border-t px-4 py-3">
          <Button variant="outline" onClick={retry}>
            {t("retry")}
          </Button>
          <Button onClick={onClose}>{t("close")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function libraryPreviewState(file: LibraryFile): PreviewState | null {
  const src = previewUrl(file)
  if (!src) return null
  const path = file.fileName
  const formatPath = file.fileExt ? `preview.${file.fileExt}` : path
  if (isImageFile(formatPath))
    return { status: "media", mediaType: "image", path, src }
  const kind = binaryPreviewKind(formatPath)
  if (kind === "pdf" || kind === "model") return { status: kind, path, src }
  if (kind === "video" || kind === "audio")
    return { status: "media", mediaType: kind, path, src }
  return { status: "url", path, src: remoteArtifactFrameSrc(src, formatPath) }
}

function previewUrl(file: LibraryFile): string | null {
  for (const raw of [file.fileUrl, file.tosUrl]) {
    try {
      const url = new URL(raw)
      if (url.protocol === "https:" || url.protocol === "http:") return url.href
    } catch {
      // 忽略无效地址，继续使用可访问的备用地址。
    }
  }
  return null
}
