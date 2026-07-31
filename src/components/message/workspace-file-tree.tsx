"use client"

import { useCallback, useState } from "react"
import { AlertCircle, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  FileTree,
  FileTreeFile,
  FileTreeFolder,
} from "@/components/ai-elements/file-tree"
import type { WorkspaceTreeState } from "@/components/message/workspace-file-tree-data"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { joinFsPath } from "@/lib/path-utils"
import { isLocalDesktop, revealItemInDir } from "@/lib/platform"
import type { FileTreeNode } from "@/lib/types"

function TreeLoadingRows() {
  return (
    <div className="w-full space-y-2.5 p-3" aria-hidden="true">
      {[72, 88, 64, 80, 56].map((width) => (
        <div key={width} className="flex items-center gap-2">
          <Skeleton className="size-4 shrink-0 rounded-sm" />
          <Skeleton className="h-3 rounded-sm" style={{ width: `${width}%` }} />
        </div>
      ))}
    </div>
  )
}

/** Wraps a tree item with a right-click context menu when `onReveal` is provided. */
function WithRevealMenu({
  path,
  label,
  onReveal,
  children,
}: {
  path: string
  label: string
  onReveal: ((path: string) => void) | undefined
  children: React.ReactNode
}) {
  if (!onReveal) return <>{children}</>
  return (
    <ContextMenu>
      <ContextMenuTrigger>{children}</ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onSelect={() => onReveal(path)}>
          {label}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

interface WorkspaceTreeNodesProps {
  nodes: FileTreeNode[]
  loadedPaths: Set<string>
  loadingPaths: Set<string>
  pathErrors: Map<string, string>
  emptyLabel: string
  loadingLabel: string
  errorLabel: string
  onReveal?: (path: string) => void
  revealLabel: string
}

function WorkspaceTreeNodes(props: WorkspaceTreeNodesProps) {
  const {
    nodes,
    loadedPaths,
    loadingPaths,
    pathErrors,
    onReveal,
    revealLabel,
  } = props
  return nodes.map((node) => {
    if (node.kind === "file") {
      return (
        <WithRevealMenu
          key={node.path}
          path={node.path}
          label={revealLabel}
          onReveal={onReveal}
        >
          <FileTreeFile path={node.path} name={node.name} />
        </WithRevealMenu>
      )
    }
    const loading = loadingPaths.has(node.path)
    const error = pathErrors.get(node.path)
    const empty = loadedPaths.has(node.path) && node.children.length === 0
    return (
      <WithRevealMenu
        key={node.path}
        path={node.path}
        label={revealLabel}
        onReveal={onReveal}
      >
        <FileTreeFolder path={node.path} name={node.name}>
          {loading ? (
            <div className="flex items-center gap-2 px-2 py-1.5 text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              <span>{props.loadingLabel}</span>
            </div>
          ) : error ? (
            <div className="px-2 py-1.5 text-destructive" title={error}>
              {props.errorLabel}
            </div>
          ) : empty ? (
            <div className="px-2 py-1.5 text-muted-foreground">
              {props.emptyLabel}
            </div>
          ) : (
            <WorkspaceTreeNodes {...props} nodes={node.children} />
          )}
        </FileTreeFolder>
      </WithRevealMenu>
    )
  })
}

interface WorkspaceTreePaneProps extends WorkspaceTreeState {
  rootPath: string
  selectedPath?: string
  onSelect: (path: string) => void
}

function TreePaneContent({
  props,
  expanded,
  onExpandedChange,
  onReveal,
  revealLabel,
}: {
  props: WorkspaceTreePaneProps
  expanded: Set<string>
  onExpandedChange: (next: Set<string>) => void
  onReveal?: (path: string) => void
  revealLabel: string
}) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  if (props.loading) return <TreeLoadingRows />
  if (props.error) {
    return (
      <div className="m-auto flex flex-col items-center gap-2 px-4 text-center">
        <AlertCircle className="size-5 text-destructive/80" />
        <p className="text-sm font-medium">{t("treeError")}</p>
        <p className="break-words text-xs text-muted-foreground">
          {props.error}
        </p>
      </div>
    )
  }
  return (
    <FileTree
      expanded={expanded}
      selectedPath={props.selectedPath}
      onExpandedChange={onExpandedChange}
      onSelect={props.onSelect}
      className="border-0 bg-transparent text-xs"
    >
      <WorkspaceTreeNodes
        nodes={props.nodes}
        loadedPaths={props.loadedPaths}
        loadingPaths={props.loadingPaths}
        pathErrors={props.pathErrors}
        emptyLabel={t("emptyFolder")}
        loadingLabel={t("loadingTree")}
        errorLabel={t("folderLoadError")}
        onReveal={onReveal}
        revealLabel={revealLabel}
      />
    </FileTree>
  )
}

export function WorkspaceTreePane(props: WorkspaceTreePaneProps) {
  const t = useTranslations("Folder.chat.workspaceFiles")
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const { loadedPaths, loadDirectory } = props

  const canReveal = isLocalDesktop()

  // Platform-appropriate label for "reveal in file manager"
  const revealLabel = (() => {
    if (!canReveal) return ""
    if (typeof navigator === "undefined") return t("openInFileManager")
    const platform =
      `${navigator.platform} ${navigator.userAgent}`.toLowerCase()
    if (platform.includes("mac")) return t("openInFinder")
    if (platform.includes("win")) return t("openInExplorer")
    return t("openInFileManager")
  })()

  const handleReveal = useCallback(
    (relativePath: string) => {
      const path = joinFsPath(props.rootPath, relativePath)
      void revealItemInDir(path).catch(() => {
        // Reveal failures are usually benign (the target may have been deleted).
      })
    },
    [props.rootPath]
  )

  const handleExpandedChange = useCallback(
    (next: Set<string>) => {
      for (const path of next) {
        if (!expanded.has(path) && !loadedPaths.has(path)) {
          void loadDirectory(path)
        }
      }
      setExpanded(next)
    },
    [expanded, loadedPaths, loadDirectory]
  )

  return (
    <section
      aria-label={t("treeLabel")}
      aria-busy={props.loading}
      className="flex min-h-0 overflow-auto border-b bg-muted/10 p-2 pr-12 md:border-r md:border-b-0 md:pr-2"
    >
      <TreePaneContent
        props={props}
        expanded={expanded}
        onExpandedChange={handleExpandedChange}
        onReveal={canReveal ? handleReveal : undefined}
        revealLabel={revealLabel}
      />
    </section>
  )
}
