"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { AlertCircle, FolderTree, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  FileTree,
  FileTreeFile,
  FileTreeFolder,
} from "@/components/ai-elements/file-tree"
import { CollapsedOverlayChip } from "@/components/chat/collapsed-overlay-chip"
import { WorkspaceFilePreview } from "@/components/message/workspace-file-preview"
import type { PreviewState } from "@/components/message/workspace-file-preview"
import { toImageDataUrl } from "@/components/message/workspace-file-preview"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { toErrorMessage } from "@/lib/app-error"
import {
  getFileTree,
  readFilePreview,
  readWorkspaceFileBase64,
} from "@/lib/api"
import { isImageFile, isOfficePreviewable } from "@/lib/language-detect"
import type { FileTreeNode } from "@/lib/types"

function collectFilePaths(nodes: FileTreeNode[], paths = new Set<string>()) {
  for (const node of nodes) {
    if (node.kind === "file") paths.add(node.path)
    else collectFilePaths(node.children, paths)
  }
  return paths
}

function WorkspaceTreeNodes({ nodes }: { nodes: FileTreeNode[] }) {
  return nodes.map((node) =>
    node.kind === "dir" ? (
      <FileTreeFolder key={node.path} path={node.path} name={node.name}>
        <WorkspaceTreeNodes nodes={node.children} />
      </FileTreeFolder>
    ) : (
      <FileTreeFile key={node.path} path={node.path} name={node.name} />
    )
  )
}

function useWorkspaceTree(rootPath: string) {
  const [nodes, setNodes] = useState<FileTreeNode[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    getFileTree(rootPath)
      .then((tree) => {
        if (!cancelled) setNodes(tree)
      })
      .catch((reason) => {
        if (!cancelled) setError(toErrorMessage(reason))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [rootPath])

  return { nodes, loading, error }
}

function useWorkspacePreview(rootPath: string, nodes: FileTreeNode[]) {
  const [preview, setPreview] = useState<PreviewState>({ status: "idle" })
  const requestId = useRef(0)
  const filePaths = useMemo(() => collectFilePaths(nodes), [nodes])

  const selectFile = useCallback(
    async (path: string) => {
      if (!filePaths.has(path)) return
      const request = (requestId.current += 1)
      setPreview({ status: "loading", path })
      try {
        if (isImageFile(path)) {
          const base64 = await readWorkspaceFileBase64(rootPath, path)
          if (request !== requestId.current) return
          setPreview({
            status: "image",
            path,
            content: toImageDataUrl(path, base64),
          })
          return
        }
        if (isOfficePreviewable(path)) {
          setPreview({ status: "office", path })
          return
        }
        const result = await readFilePreview(rootPath, path)
        if (request !== requestId.current) return
        setPreview({ status: "text", path, content: result.content })
      } catch (reason) {
        if (request !== requestId.current) return
        setPreview({ status: "error", path, message: toErrorMessage(reason) })
      }
    },
    [filePaths, rootPath]
  )

  return { preview, selectFile }
}

interface WorkspaceTreePaneProps {
  nodes: FileTreeNode[]
  loading: boolean
  error: string | null
  selectedPath?: string
  onSelect: (path: string) => void
}

function WorkspaceTreePane(props: WorkspaceTreePaneProps) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const { nodes, loading, error, selectedPath, onSelect } = props

  let content
  if (loading) {
    content = (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        {t("loadingTree")}
      </div>
    )
  } else if (error) {
    content = (
      <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
        <AlertCircle className="size-5 text-destructive/80" />
        <p className="text-sm font-medium">{t("treeError")}</p>
        <p className="break-words text-xs text-muted-foreground">{error}</p>
      </div>
    )
  } else if (nodes.length === 0) {
    content = (
      <p className="m-auto text-sm text-muted-foreground">{t("empty")}</p>
    )
  } else {
    content = (
      <FileTree
        expanded={expanded}
        selectedPath={selectedPath}
        onExpandedChange={setExpanded}
        onSelect={onSelect}
        className="border-0 bg-transparent text-xs"
      >
        <WorkspaceTreeNodes nodes={nodes} />
      </FileTree>
    )
  }

  return (
    <section
      aria-label={t("treeLabel")}
      className="flex min-h-0 overflow-auto border-b bg-muted/15 p-2 md:border-r md:border-b-0"
    >
      {content}
    </section>
  )
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
  return (
    <section
      aria-label={t("previewLabel")}
      className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] bg-background"
    >
      <div className="flex h-9 min-w-0 items-center border-b bg-muted/20 px-3">
        <span
          className="truncate font-mono text-xs text-muted-foreground"
          title={path ?? undefined}
        >
          {path ?? t("previewLabel")}
        </span>
      </div>
      <div className="min-h-0">
        <WorkspaceFilePreview state={preview} rootPath={rootPath} />
      </div>
    </section>
  )
}

function WorkspaceFilesDialogContent({ rootPath }: { rootPath: string }) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const tree = useWorkspaceTree(rootPath)
  const { preview, selectFile } = useWorkspacePreview(rootPath, tree.nodes)
  const selectedPath = preview.status === "idle" ? undefined : preview.path

  return (
    <DialogContent className="grid h-[min(46rem,calc(100dvh-2rem))] max-w-[min(72rem,calc(100vw-2rem))] grid-rows-[auto_minmax(0,1fr)] gap-0 overflow-hidden rounded-lg p-0 sm:max-w-[min(72rem,calc(100vw-2rem))]">
      <DialogHeader className="border-b px-5 py-4">
        <div className="flex min-w-0 items-center gap-2">
          <FolderTree className="size-4 shrink-0 text-muted-foreground" />
          <DialogTitle className="truncate text-base">{t("title")}</DialogTitle>
        </div>
        <DialogDescription className="sr-only">
          {t("description")}
        </DialogDescription>
        <p
          className="truncate font-mono text-xs text-muted-foreground"
          title={rootPath}
        >
          {rootPath}
        </p>
      </DialogHeader>
      <div className="grid min-h-0 grid-rows-[minmax(10rem,2fr)_minmax(12rem,3fr)] md:grid-cols-[minmax(13rem,17rem)_minmax(0,1fr)] md:grid-rows-1">
        <WorkspaceTreePane
          {...tree}
          selectedPath={selectedPath}
          onSelect={(path) => void selectFile(path)}
        />
        <WorkspacePreviewPane preview={preview} rootPath={rootPath} />
      </div>
    </DialogContent>
  )
}

export function WorkspaceFilesDialog() {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const { activeFolder } = useActiveFolder()
  const [open, setOpen] = useState(false)
  const rootPath = activeFolder?.path ?? null
  if (!rootPath) return null

  return (
    <>
      <CollapsedOverlayChip
        icon={<FolderTree className="size-3" />}
        summary={t("open")}
        onClick={() => setOpen(true)}
      />
      <Dialog open={open} onOpenChange={setOpen}>
        {open && <WorkspaceFilesDialogContent rootPath={rootPath} />}
      </Dialog>
    </>
  )
}
