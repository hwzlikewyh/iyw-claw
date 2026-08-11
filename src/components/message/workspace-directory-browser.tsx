"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { FileCode2, Files } from "lucide-react"
import { useTranslations } from "next-intl"

import { WorkspaceFilePreview } from "@/components/message/workspace-file-preview"
import type { PreviewState } from "@/components/message/workspace-file-preview"
import { useLazyWorkspaceTree } from "@/components/message/workspace-file-tree-data"
import { WorkspaceTreePane } from "@/components/message/workspace-file-tree"
import {
  getCachedWorkspacePreview,
  loadWorkspacePreview,
} from "@/components/message/workspace-file-preview-loader"
import { toErrorMessage } from "@/lib/app-error"
import { isOfficePreviewable } from "@/lib/language-detect"
import type { FileTreeNode } from "@/lib/types"
import { cn } from "@/lib/utils"

interface WorkspaceDirectoryBrowserProps {
  rootPath: string
  className?: string
}

function collectFilePaths(nodes: FileTreeNode[], paths = new Set<string>()) {
  for (const node of nodes) {
    if (node.kind === "file") paths.add(node.path)
    else collectFilePaths(node.children, paths)
  }
  return paths
}

function useDirectoryPreview(rootPath: string, nodes: FileTreeNode[]) {
  const [preview, setPreview] = useState<PreviewState>({ status: "idle" })
  const requestId = useRef(0)
  const filePaths = useMemo(() => collectFilePaths(nodes), [nodes])

  const selectFile = useCallback(
    async (path: string) => {
      if (!filePaths.has(path)) return
      const request = (requestId.current += 1)
      const cached = getCachedWorkspacePreview(rootPath, path)
      if (cached) {
        setPreview(cached)
        return
      }
      if (isOfficePreviewable(path)) {
        setPreview({ status: "office", path })
        return
      }
      setPreview({ status: "loading", path })
      try {
        const next = await loadWorkspacePreview(rootPath, path)
        if (request !== requestId.current) return
        setPreview(next)
      } catch (reason) {
        if (request !== requestId.current) return
        const message = toErrorMessage(reason)
        console.error("[workspace-files] preview load failed", {
          relativePath: path,
          errorType: reason instanceof Error ? reason.name : typeof reason,
        })
        setPreview({ status: "error", path, message })
      }
    },
    [filePaths, rootPath]
  )

  useEffect(() => () => void (requestId.current += 1), [])

  return { preview, selectFile }
}

function WorkspacePreviewPane({
  preview,
  rootPath,
}: {
  preview: PreviewState
  rootPath: string
}) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const path = preview.status === "idle" ? null : preview.path
  const name = path?.split(/[/\\]/).pop()
  return (
    <section
      aria-label={t("previewLabel")}
      className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] bg-background"
    >
      <div className="flex h-11 min-w-0 items-center gap-2 border-b bg-muted/15 px-3 pr-12">
        <FileCode2 className="size-4 shrink-0 text-muted-foreground" />
        <span
          className="truncate text-sm font-medium text-foreground/90"
          title={path ?? undefined}
        >
          {name ?? t("previewLabel")}
        </span>
      </div>
      <div className="min-h-0">
        <WorkspaceFilePreview state={preview} rootPath={rootPath} />
      </div>
    </section>
  )
}

function EmptyWorkspaceState() {
  const t = useTranslations("Folder.chat.workspaceFiles")
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
      <div className="grid size-10 place-items-center rounded-md bg-muted/60 text-muted-foreground">
        <Files className="size-5" />
      </div>
      <div className="space-y-1">
        <p className="text-sm font-medium">{t("empty")}</p>
        <p className="text-xs text-muted-foreground">{t("emptyHint")}</p>
      </div>
    </div>
  )
}

export function WorkspaceDirectoryBrowser({
  rootPath,
  className,
}: WorkspaceDirectoryBrowserProps) {
  return (
    <WorkspaceDirectoryBrowserContent
      key={rootPath}
      rootPath={rootPath}
      className={className}
    />
  )
}

function WorkspaceDirectoryBrowserContent({
  rootPath,
  className,
}: WorkspaceDirectoryBrowserProps) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const tree = useLazyWorkspaceTree(rootPath)
  const { preview, selectFile } = useDirectoryPreview(rootPath, tree.nodes)
  const selectedPath = preview.status === "idle" ? undefined : preview.path
  const empty = !tree.loading && !tree.error && tree.nodes.length === 0
  const pathErrors = useMemo(
    () =>
      new Map(
        [...tree.pathErrors.keys()].map((path) => [path, t("folderLoadError")])
      ),
    [t, tree.pathErrors]
  )

  if (empty) {
    return (
      <div className={cn("h-full", className)}>
        <EmptyWorkspaceState />
      </div>
    )
  }

  return (
    <div
      className={cn(
        "grid min-h-0 grid-rows-[minmax(10rem,2fr)_minmax(12rem,3fr)] md:grid-cols-[minmax(13rem,17rem)_minmax(0,1fr)] md:grid-rows-1",
        className
      )}
    >
      <WorkspaceTreePane
        {...tree}
        error={tree.error ? t("treeError") : null}
        pathErrors={pathErrors}
        rootPath={rootPath}
        selectedPath={selectedPath}
        onSelect={(path) => void selectFile(path)}
      />
      <WorkspacePreviewPane preview={preview} rootPath={rootPath} />
    </div>
  )
}
