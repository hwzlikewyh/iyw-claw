"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { revealItemInDir } from "@/lib/platform"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useTabStore } from "@/contexts/tab-context"
import { useTerminalContext } from "@/contexts/terminal-context"
import {
  useWorkspaceActions,
  useWorkspaceFileTabs,
} from "@/contexts/workspace-context"
import { useWorkspaceStateStore } from "@/hooks/use-workspace-state-store"
import { findOwningFolder } from "@/lib/file-open-target"
import { AuxPanelNoFolderEmpty } from "@/components/layout/aux-panel-no-folder-empty"
import { WorkspaceDegradedBanner } from "@/components/layout/workspace-degraded-banner"
import { WorkspaceUploadDialog } from "@/components/layout/workspace-upload-dialog"
import {
  createFileTreeEntry,
  deleteFileTreeEntry,
  downloadWorkspaceDir,
  downloadWorkspaceFile,
  getFileTree,
  renameFileTreeEntry,
  WORKSPACE_DOWNLOAD_CANCELLED,
} from "@/lib/api"
import { isDesktop, isRemoteDesktopMode } from "@/lib/transport"
import { emitAttachFileToSession } from "@/lib/session-attachment-events"
import { ScrollArea } from "@/components/ui/scroll-area"
import type { FileTreeNode } from "@/lib/types"
import {
  FileTree,
  FileTreeFolder,
  FileTreeFile,
} from "@/components/ai-elements/file-tree"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { joinFsPath } from "@/lib/path-utils"
import { toErrorMessage } from "@/lib/app-error"
import { copyTextFromMenu } from "@/lib/utils"

function parentDir(filePath: string): string {
  const slashIndex = filePath.lastIndexOf("/")
  const backslashIndex = filePath.lastIndexOf("\\")
  const splitIndex = Math.max(slashIndex, backslashIndex)
  // No separator at all: the input is a leaf living at its root. For an
  // OS path that's a degenerate "C:" / "foo" — we can't navigate above
  // it, so the caller treated the result as the path itself. For a
  // workspace-relative path like "README.md" the answer is "workspace
  // root", encoded as empty string. The empty-string convention is the
  // safer default and matches what every caller currently expects.
  if (splitIndex < 0) return ""
  if (splitIndex === 0) return filePath.slice(0, 1)
  return filePath.slice(0, splitIndex)
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() || path
}

async function copyPathToClipboard(
  absolutePath: string,
  messages: { success: string; failure: string }
) {
  // copyTextFromMenu defers the write until this context menu has closed, so
  // the execCommand clipboard fallback works in non-secure web contexts.
  const ok = await copyTextFromMenu(absolutePath)
  if (ok) {
    toast.success(messages.success)
  } else {
    toast.error(messages.failure)
  }
}

const FILE_TREE_ROOT_PATH = "__workspace_root__"

interface FileActionTarget {
  kind: "file" | "dir"
  path: string
  name: string
}

function normalizeComparePath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "")
}

function prefixFileTreeNodePaths(
  nodes: FileTreeNode[],
  prefix: string
): FileTreeNode[] {
  return nodes.map((node) => {
    const nextPath = prefix ? `${prefix}/${node.path}` : node.path
    if (node.kind === "file") {
      return {
        ...node,
        path: nextPath,
      }
    }
    return {
      ...node,
      path: nextPath,
      children: prefixFileTreeNodePaths(node.children, nextPath),
    }
  })
}

function applyLazyTreeOverrides(
  nodes: FileTreeNode[],
  overrides: ReadonlyMap<string, FileTreeNode[]>
): FileTreeNode[] {
  return nodes.map((node) => {
    if (node.kind === "file") return node
    const overrideChildren = overrides.get(node.path)
    const baseChildren = overrideChildren ?? node.children
    return {
      ...node,
      children: applyLazyTreeOverrides(baseChildren, overrides),
    }
  })
}

function findDirectoryChildren(
  nodes: FileTreeNode[],
  targetPath: string
): FileTreeNode[] | null {
  for (const node of nodes) {
    if (node.kind !== "dir") continue
    if (normalizeComparePath(node.path) === targetPath) {
      return node.children
    }
    const nested = findDirectoryChildren(node.children, targetPath)
    if (nested) return nested
  }
  return null
}

interface RenderNodeProps {
  node: FileTreeNode
  expandedPaths: ReadonlySet<string>
  workspacePath: string
  activeSessionTabId: string | null
  webMode: boolean
  folderUploadSupported: boolean
  onOpenFilePreview: (path: string) => void
  onOpenDirInTerminal: (dirPath: string, fileName: string) => Promise<void>
  onRequestRename: (target: FileActionTarget) => void
  onRequestCreate: (parentPath: string, kind: "file" | "dir") => void
  onRequestDelete: (target: FileActionTarget) => void
  onRequestUpload: (targetPath: string) => void
  onRequestDownloadFile: (target: FileActionTarget) => void
  onRequestDownloadDir: (target: FileActionTarget) => void
  onRefresh: () => void
}

function RenderNode({
  node,
  expandedPaths,
  workspacePath,
  activeSessionTabId,
  webMode,
  folderUploadSupported,
  onOpenFilePreview,
  onOpenDirInTerminal,
  onRequestCreate,
  onRequestRename,
  onRequestDelete,
  onRequestUpload,
  onRequestDownloadFile,
  onRequestDownloadDir,
  onRefresh,
}: RenderNodeProps) {
  const t = useTranslations("Folder.fileTreeTab")
  const tCommon = useTranslations("Folder.common")

  const systemExplorerLabel =
    typeof navigator === "undefined"
      ? t("openInFileManager")
      : (() => {
          const platform =
            `${navigator.platform} ${navigator.userAgent}`.toLowerCase()
          if (platform.includes("mac")) return t("openInFinder")
          if (platform.includes("win")) return t("openInExplorer")
          return t("openInFileManager")
        })()

  if (node.kind === "file") {
    const absolutePath = joinFsPath(workspacePath, node.path)
    const dirPath = parentDir(absolutePath)

    const handleAttachToSession = () => {
      if (!activeSessionTabId) return
      emitAttachFileToSession({
        tabId: activeSessionTabId,
        path: absolutePath,
      })
    }

    const handleOpenInSystemExplorer = async () => {
      try {
        await revealItemInDir(absolutePath)
      } catch (error) {
        const message = toErrorMessage(error)
        toast.error(t("toasts.openDirectoryFailed"), { description: message })
      }
    }

    return (
      <ContextMenu>
        <ContextMenuTrigger>
          <FileTreeFile path={node.path} name={node.name} />
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuItem onSelect={() => onOpenFilePreview(node.path)}>
            {tCommon("openFile")}
          </ContextMenuItem>
          <ContextMenuItem
            onSelect={() => void handleAttachToSession()}
            disabled={!activeSessionTabId}
          >
            {t("attachToCurrentSession")}
          </ContextMenuItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger>{t("new")}</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem
                onSelect={() => onRequestCreate(node.path, "file")}
              >
                {t("newFile")}
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() => onRequestCreate(node.path, "dir")}
              >
                {t("newDirectory")}
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuItem onSelect={() => onRequestRename(node)}>
            {tCommon("rename")}
          </ContextMenuItem>
          <ContextMenuItem onSelect={onRefresh}>
            {t("reloadFromDisk")}
          </ContextMenuItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger>{t("openIn")}</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem
                onSelect={() => void handleOpenInSystemExplorer()}
              >
                {systemExplorerLabel}
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() => void onOpenDirInTerminal(dirPath, node.name)}
              >
                {t("openInTerminal")}
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuItem
            onSelect={() =>
              void copyPathToClipboard(absolutePath, {
                success: t("toasts.pathCopied"),
                failure: t("toasts.copyPathFailed"),
              })
            }
          >
            {t("copyPath")}
          </ContextMenuItem>
          {webMode && (
            <>
              <ContextMenuItem
                onSelect={() => onRequestUpload(parentDir(node.path))}
              >
                {t("upload")}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => onRequestDownloadFile(node)}>
                {t("download")}
              </ContextMenuItem>
            </>
          )}
          <ContextMenuItem
            onSelect={() => onRequestDelete(node)}
            variant="destructive"
          >
            {tCommon("delete")}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
    )
  }

  const absolutePath = joinFsPath(workspacePath, node.path)
  const shouldRenderChildren = expandedPaths.has(node.path)

  const handleAttachDirToSession = () => {
    if (!activeSessionTabId) return
    emitAttachFileToSession({
      tabId: activeSessionTabId,
      path: absolutePath,
    })
  }

  const handleOpenDirInSystemExplorer = async () => {
    try {
      await revealItemInDir(absolutePath)
    } catch (error) {
      const message = toErrorMessage(error)
      toast.error(t("toasts.openDirectoryFailed"), { description: message })
    }
  }

  return (
    <ContextMenu>
      <ContextMenuTrigger>
        <FileTreeFolder path={node.path} name={node.name}>
          {shouldRenderChildren
            ? node.children.map((child) => (
                <RenderNode
                  key={child.path}
                  node={child}
                  expandedPaths={expandedPaths}
                  workspacePath={workspacePath}
                  activeSessionTabId={activeSessionTabId}
                  webMode={webMode}
                  folderUploadSupported={folderUploadSupported}
                  onOpenFilePreview={onOpenFilePreview}
                  onOpenDirInTerminal={onOpenDirInTerminal}
                  onRequestCreate={onRequestCreate}
                  onRequestRename={onRequestRename}
                  onRequestDelete={onRequestDelete}
                  onRequestUpload={onRequestUpload}
                  onRequestDownloadFile={onRequestDownloadFile}
                  onRequestDownloadDir={onRequestDownloadDir}
                  onRefresh={onRefresh}
                />
              ))
            : null}
        </FileTreeFolder>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem
          onSelect={handleAttachDirToSession}
          disabled={!activeSessionTabId}
        >
          {t("attachToCurrentSession")}
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>{t("new")}</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            <ContextMenuItem
              onSelect={() => onRequestCreate(node.path, "file")}
            >
              {t("newFile")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => onRequestCreate(node.path, "dir")}>
              {t("newDirectory")}
            </ContextMenuItem>
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuItem onSelect={() => onRequestRename(node)}>
          {tCommon("rename")}
        </ContextMenuItem>
        <ContextMenuSub>
          <ContextMenuSubTrigger>{t("openIn")}</ContextMenuSubTrigger>
          <ContextMenuSubContent>
            <ContextMenuItem
              onSelect={() => void handleOpenDirInSystemExplorer()}
            >
              {systemExplorerLabel}
            </ContextMenuItem>
            <ContextMenuItem
              onSelect={() => void onOpenDirInTerminal(absolutePath, node.name)}
            >
              {t("openInTerminal")}
            </ContextMenuItem>
          </ContextMenuSubContent>
        </ContextMenuSub>
        <ContextMenuItem
          onSelect={() =>
            void copyPathToClipboard(absolutePath, {
              success: t("toasts.pathCopied"),
              failure: t("toasts.copyPathFailed"),
            })
          }
        >
          {t("copyPath")}
        </ContextMenuItem>
        {webMode && (
          <>
            <ContextMenuItem onSelect={() => onRequestUpload(node.path)}>
              {t("upload")}
            </ContextMenuItem>
            <ContextMenuItem onSelect={() => onRequestDownloadDir(node)}>
              {t("downloadAsZip")}
            </ContextMenuItem>
          </>
        )}
        <ContextMenuItem onSelect={onRefresh}>
          {t("reloadFromDisk")}
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={() => onRequestDelete(node)}
          variant="destructive"
        >
          {tCommon("delete")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

export function FileTreeTab() {
  const t = useTranslations("Folder.fileTreeTab")
  const tCommon = useTranslations("Folder.common")
  const { pendingRevealPath, consumePendingRevealPath } = useAuxPanelContext()
  const { activeFolder: folder } = useActiveFolder()
  const tabs = useTabStore((s) => s.tabs)
  const activeTabId = useTabStore((s) => s.activeTabId)
  const { createTerminalInDirectory } = useTerminalContext()
  const { activeFilePath } = useWorkspaceFileTabs()
  const { openFilePreview } = useWorkspaceActions()
  // File tab paths are absolute; the tree's node paths are relative to
  // THIS panel's folder — derive the relative form (undefined when the
  // active file lives outside this folder, which correctly unselects).
  const selectedTreePath = useMemo(() => {
    if (!activeFilePath || !folder) return undefined
    return (
      findOwningFolder(activeFilePath, [{ id: folder.id, path: folder.path }])
        ?.relPath ?? undefined
    )
  }, [activeFilePath, folder])
  const workspaceState = useWorkspaceStateStore(folder?.path ?? null)
  const [nodes, setNodes] = useState<FileTreeNode[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [renameTarget, setRenameTarget] = useState<FileActionTarget | null>(
    null
  )
  const [renameValue, setRenameValue] = useState("")
  const [renaming, setRenaming] = useState(false)
  const [createParentPath, setCreateParentPath] = useState<string | null>(null)
  const [createKind, setCreateKind] = useState<"file" | "dir">("file")
  const [createName, setCreateName] = useState("")
  const [creating, setCreating] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<FileActionTarget | null>(
    null
  )
  const [deleting, setDeleting] = useState(false)
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(
    () => new Set([FILE_TREE_ROOT_PATH])
  )
  const previousExpandedPathsRef = useRef<Set<string>>(
    new Set([FILE_TREE_ROOT_PATH])
  )
  const lazyLoadedChildrenByPathRef = useRef<Map<string, FileTreeNode[]>>(
    new Map()
  )
  const lazyLoadingDirPathsRef = useRef<Set<string>>(new Set())
  const loadDirectoryChildrenRef = useRef<
    ((dirPath: string) => Promise<void>) | null
  >(null)
  const expandedPathsRef = useRef<Set<string>>(new Set([FILE_TREE_ROOT_PATH]))
  const workspaceTreeRef = useRef<FileTreeNode[]>([])

  useEffect(() => {
    setExpandedPaths(new Set([FILE_TREE_ROOT_PATH]))
    previousExpandedPathsRef.current = new Set([FILE_TREE_ROOT_PATH])
    lazyLoadedChildrenByPathRef.current.clear()
    lazyLoadingDirPathsRef.current.clear()
  }, [folder?.path])

  // Handle pending reveal path: expand all ancestor directories once tree is loaded
  const hasNodes = nodes.length > 0
  useEffect(() => {
    if (!pendingRevealPath || !hasNodes) return
    consumePendingRevealPath()
    setExpandedPaths((prev) => {
      const next = new Set(prev)
      next.add(FILE_TREE_ROOT_PATH)
      let idx = pendingRevealPath.indexOf("/")
      while (idx !== -1) {
        next.add(pendingRevealPath.slice(0, idx))
        idx = pendingRevealPath.indexOf("/", idx + 1)
      }
      next.add(pendingRevealPath)
      return next
    })
  }, [pendingRevealPath, consumePendingRevealPath, hasNodes])

  const activeSessionTabId = useMemo(() => {
    const activeTab = tabs.find((tab) => tab.id === activeTabId)
    if (!activeTab) return null
    if (activeTab.kind !== "conversation") {
      return null
    }
    return activeTab.id
  }, [tabs, activeTabId])

  const fetchTree = useCallback(async () => {
    if (!folder?.path) {
      setNodes([])
      setLoading(false)
      setError(null)
      return
    }

    // Drop the lazy-load override cache so the fresh snapshot is not
    // masked by stale children (e.g. after deletes, renames, or files the
    // agent just created). Reading expanded paths via a ref
    // keeps fetchTree's identity stable across expand/collapse so
    // downstream memoization is not invalidated on every tree interaction.
    const pathsToReload = Array.from(expandedPathsRef.current).filter(
      (path) => path !== FILE_TREE_ROOT_PATH
    )
    lazyLoadedChildrenByPathRef.current.clear()
    await workspaceState.requestResync("manual_refresh")
    // Re-hydrate children for directories beyond WORKSPACE_TREE_MAX_DEPTH
    // that are still expanded — the backend snapshot does not include them.
    const loader = loadDirectoryChildrenRef.current
    if (loader) {
      for (const path of pathsToReload) {
        void loader(path)
      }
    }
  }, [folder?.path, workspaceState])

  // Tree updates are the only source that should cause a full setNodes.
  // applyLazyTreeOverrides rebuilds every directory node object, which forces
  // React to re-render the entire tree. Keeping this effect narrow avoids
  // wasted work on health / seq / error transitions that don't touch
  // the tree shape (e.g. the intermediate "resyncing" patch during a refresh).
  useEffect(() => {
    workspaceTreeRef.current = workspaceState.tree
    setNodes(
      applyLazyTreeOverrides(
        workspaceState.tree,
        lazyLoadedChildrenByPathRef.current
      )
    )
  }, [folder?.path, workspaceState.tree])

  useEffect(() => {
    setLoading(
      workspaceState.health === "resyncing" && workspaceState.seq === 0
    )
    setError(workspaceState.health === "degraded" ? workspaceState.error : null)
  }, [workspaceState.error, workspaceState.health, workspaceState.seq])

  const loadDirectoryChildren = useCallback(
    async (dirPath: string) => {
      const rootPath = folder?.path
      if (!rootPath) return
      const normalizedDirPath = normalizeComparePath(dirPath)
      if (!normalizedDirPath) return
      if (lazyLoadedChildrenByPathRef.current.has(normalizedDirPath)) return
      if (lazyLoadingDirPathsRef.current.has(normalizedDirPath)) return

      // Check the backend tree (source of truth), not the rendered `nodes`.
      // `nodes` carries stale lazy-cache overrides that don't invalidate
      // until a tree_replace delta arrives — but for directories beyond
      // WORKSPACE_TREE_MAX_DEPTH the backend never emits tree_replace for
      // changes inside them (their children are not in tree_snapshot, so
      // the refreshed tree compares equal to the old one). Checking
      // `nodes` would cause fetchTree's forced reload to short-circuit on
      // the stale override and miss deletions / creations in deep dirs.
      const existingChildren = findDirectoryChildren(
        workspaceTreeRef.current,
        normalizedDirPath
      )
      if (existingChildren && existingChildren.length > 0) {
        return
      }

      lazyLoadingDirPathsRef.current.add(normalizedDirPath)
      try {
        const subtree = await getFileTree(
          joinFsPath(rootPath, normalizedDirPath),
          1
        )
        const prefixed = prefixFileTreeNodePaths(subtree, normalizedDirPath)
        lazyLoadedChildrenByPathRef.current.set(normalizedDirPath, prefixed)
        setNodes((prev) =>
          applyLazyTreeOverrides(prev, lazyLoadedChildrenByPathRef.current)
        )
      } catch {
        // Ignore lazy load failures and keep current collapsed/empty state.
      } finally {
        lazyLoadingDirPathsRef.current.delete(normalizedDirPath)
      }
    },
    [folder?.path]
  )

  useEffect(() => {
    loadDirectoryChildrenRef.current = loadDirectoryChildren
  }, [loadDirectoryChildren])

  useEffect(() => {
    expandedPathsRef.current = expandedPaths
  }, [expandedPaths])

  // Subscribe to workspace envelopes to invalidate lazy-loaded overrides for
  // directories beyond WORKSPACE_TREE_MAX_DEPTH. Those directories are never
  // reflected in the backend's depth-2 tree_snapshot, so changes inside them
  // don't emit a tree_replace delta — the frontend has to target invalidation
  // by matching each `changed_paths` entry against its cached ancestors.
  // The backend already debounces raw FS events (300ms / 1.5s max), so we only
  // need a microtask hop here to merge paths that hit the same cached
  // ancestor within one envelope (or any synchronous burst of envelopes).
  const subscribeWorkspaceEnvelopes = workspaceState.subscribeEnvelopes
  useEffect(() => {
    if (!subscribeWorkspaceEnvelopes) return

    const pendingPaths = new Set<string>()
    let flushScheduled = false
    let disposed = false

    const flushPending = () => {
      flushScheduled = false
      if (disposed || pendingPaths.size === 0) return
      const paths = Array.from(pendingPaths)
      pendingPaths.clear()

      const loader = loadDirectoryChildrenRef.current
      const cache = lazyLoadedChildrenByPathRef.current
      const invalidated = new Set<string>()

      for (const changed of paths) {
        const normalized = normalizeComparePath(changed)
        if (!normalized) continue
        // When the changed path is itself a cached directory (FS events
        // that report the directory directly, e.g. a rename or a dir-level
        // notification), its own entry is stale — invalidate it.
        if (cache.has(normalized)) {
          invalidated.add(normalized)
        }
        // Independently of the above, walk up to the nearest cached
        // ancestor: the ancestor's children listing may also be stale
        // (a child was added, removed, or renamed). Without this, cases
        // where both a parent and child are cached leave the parent
        // holding a ghost reference to the old child.
        let cursor = normalized
        while (cursor.length > 0) {
          const slash = cursor.lastIndexOf("/")
          const parent = slash === -1 ? "" : cursor.slice(0, slash)
          if (parent.length === 0) break
          if (cache.has(parent)) {
            invalidated.add(parent)
            break
          }
          cursor = parent
        }
      }

      if (invalidated.size === 0) return
      for (const path of invalidated) {
        cache.delete(path)
      }
      if (!loader) return
      // Skip refetching directories that are no longer expanded — their
      // cleared cache will be re-hydrated on the next expansion via the
      // expandedPaths effect. This avoids spurious getFileTree traffic
      // for collapsed branches under bursty FS activity.
      const expanded = expandedPathsRef.current
      for (const path of invalidated) {
        if (!expanded.has(path)) continue
        void loader(path)
      }
    }

    const unsubscribe = subscribeWorkspaceEnvelopes(({ changed_paths }) => {
      if (!changed_paths || changed_paths.length === 0) return
      for (const path of changed_paths) {
        pendingPaths.add(path)
      }
      if (flushScheduled) return
      flushScheduled = true
      queueMicrotask(flushPending)
    })

    return () => {
      disposed = true
      unsubscribe()
      pendingPaths.clear()
    }
  }, [subscribeWorkspaceEnvelopes])

  useEffect(() => {
    const previousExpanded = previousExpandedPathsRef.current
    for (const path of expandedPaths) {
      if (path === FILE_TREE_ROOT_PATH) continue
      if (previousExpanded.has(path)) continue
      void loadDirectoryChildren(path)
    }
    previousExpandedPathsRef.current = new Set(expandedPaths)
  }, [expandedPaths, folder?.path, loadDirectoryChildren])

  const filePathSet = useMemo(() => {
    const paths = new Set<string>()
    const collect = (items: FileTreeNode[]) => {
      for (const item of items) {
        if (item.kind === "file") {
          paths.add(item.path)
        } else {
          collect(item.children)
        }
      }
    }
    collect(nodes)
    return paths
  }, [nodes])

  const handleTreeSelect = useCallback(
    (path: string) => {
      if (!filePathSet.has(path)) return
      void openFilePreview(path)
    },
    [filePathSet, openFilePreview]
  )

  const handleOpenDirInTerminal = useCallback(
    async (dirPath: string, fileName: string) => {
      const terminalTitle = t("terminalTitle", { name: baseName(fileName) })
      const terminalId = await createTerminalInDirectory(dirPath, terminalTitle)
      if (!terminalId) {
        toast.error(t("toasts.openBuiltinTerminalFailed"))
      }
    },
    [createTerminalInDirectory, t]
  )

  const handleRequestCreate = useCallback(
    (parentPath: string, kind: "file" | "dir") => {
      setCreateParentPath(parentPath)
      setCreateKind(kind)
      setCreateName("")
    },
    []
  )

  const handleRequestRename = useCallback((target: FileActionTarget) => {
    setRenameTarget(target)
    setRenameValue(target.name)
  }, [])

  const handleRequestDelete = useCallback((target: FileActionTarget) => {
    setDeleteTarget(target)
  }, [])

  // ─── Web upload / download (issue #179) ───
  // In web mode the user has no native file dialog, so the file-tree
  // context menu opens `WorkspaceUploadDialog`, which owns the queue,
  // progress UI, and cancellation. We only track which directory the
  // user right-clicked from and whether the dialog is open.
  const [webMode, setWebMode] = useState(false)
  // `webkitdirectory` is non-standard. Chromium, Edge, Firefox, and
  // desktop Safari support it; iOS Safari does not, and historically
  // some embedded webviews lacked it too. Feature-detect at mount and
  // hide the "Select folder" affordance where the picker would silently
  // fall back to single-file selection — that would surprise the user
  // mid-flow and risk corrupting the relative-path contract.
  const [folderUploadSupported, setFolderUploadSupported] = useState(false)
  const [uploadDialogOpen, setUploadDialogOpen] = useState(false)
  const [uploadDialogTarget, setUploadDialogTarget] = useState("")
  useEffect(() => {
    // "webMode" here is a misnomer for "needs in-app upload/download
    // affordances because there's no native OS file picker for the
    // *destination/source* filesystem". That's true in pure-web mode
    // AND in remote-desktop mode (where the workspace lives on the
    // remote server, not on the local disk the OS dialog would target).
    setWebMode(!isDesktop() || isRemoteDesktopMode())
    setFolderUploadSupported(
      "webkitdirectory" in document.createElement("input")
    )
  }, [])

  const handleRequestUpload = useCallback((targetPath: string) => {
    setUploadDialogTarget(targetPath)
    setUploadDialogOpen(true)
  }, [])

  const handleUploadComplete = useCallback(() => {
    void fetchTree()
  }, [fetchTree])

  const handleRequestDownloadFile = useCallback(
    async (target: FileActionTarget) => {
      const folderPath = folder?.path
      if (!folderPath) return
      try {
        const result = await downloadWorkspaceFile(
          folderPath,
          target.path,
          target.name
        )
        // Remote-desktop downloads flow through a save-dialog; surface
        // the cancel-vs-saved outcome instead of silently doing nothing.
        if (result.status === "started") return
        if (result.status === WORKSPACE_DOWNLOAD_CANCELLED) return
        if (result.savedPath) {
          toast.success(t("toasts.downloadSaved", { name: target.name }), {
            description: result.savedPath,
          })
        }
      } catch (error) {
        const message = toErrorMessage(error)
        toast.error(t("toasts.downloadFailed", { name: target.name }), {
          description: message,
        })
      }
    },
    [folder?.path, t]
  )

  const handleRequestDownloadDir = useCallback(
    async (target: FileActionTarget) => {
      const folderPath = folder?.path
      if (!folderPath) return
      const name = target.name || baseName(folderPath) || "workspace"
      try {
        const result = await downloadWorkspaceDir(folderPath, target.path, name)
        if (result.status === "started") return
        if (result.status === WORKSPACE_DOWNLOAD_CANCELLED) return
        if (result.savedPath) {
          toast.success(t("toasts.downloadSaved", { name }), {
            description: result.savedPath,
          })
        }
      } catch (error) {
        const message = toErrorMessage(error)
        toast.error(t("toasts.downloadFailed", { name }), {
          description: message,
        })
      }
    },
    [folder?.path, t]
  )

  const handleCreateConfirm = useCallback(async () => {
    if (!folder?.path || createParentPath === null) return
    const trimmedName = createName.trim()
    if (!trimmedName) {
      setCreateParentPath(null)
      return
    }

    setCreating(true)
    try {
      await createFileTreeEntry(
        folder.path,
        createParentPath,
        trimmedName,
        createKind
      )
      setCreateParentPath(null)
      setCreateName("")
      await fetchTree()
    } catch (error) {
      const message = toErrorMessage(error)
      toast.error(t("toasts.createFailed"), { description: message })
    } finally {
      setCreating(false)
    }
  }, [createKind, createName, createParentPath, fetchTree, folder?.path, t])

  const handleRenameConfirm = useCallback(async () => {
    if (!folder?.path || !renameTarget) return
    const nextName = renameValue.trim()
    if (!nextName || nextName === renameTarget.name) {
      setRenameTarget(null)
      return
    }

    setRenaming(true)
    try {
      await renameFileTreeEntry(folder.path, renameTarget.path, nextName)
      setRenameTarget(null)
      setRenameValue("")
      await fetchTree()
    } catch (error) {
      const message = toErrorMessage(error)
      toast.error(t("toasts.renameFailed"), { description: message })
    } finally {
      setRenaming(false)
    }
  }, [fetchTree, folder?.path, renameTarget, renameValue, t])

  const handleDeleteConfirm = useCallback(async () => {
    if (!folder?.path || !deleteTarget) return
    setDeleting(true)
    try {
      await deleteFileTreeEntry(folder.path, deleteTarget.path)
      setDeleteTarget(null)
      await fetchTree()
    } catch (error) {
      const message = toErrorMessage(error)
      toast.error(t("toasts.deleteFailed"), { description: message })
    } finally {
      setDeleting(false)
    }
  }, [deleteTarget, fetchTree, folder?.path, t])

  const rootNodeName = useMemo(() => {
    if (!folder?.path) return t("workspace")
    return baseName(folder.path)
  }, [folder?.path, t])

  const systemExplorerLabel =
    typeof navigator === "undefined"
      ? t("openInFileManager")
      : (() => {
          const platform =
            `${navigator.platform} ${navigator.userAgent}`.toLowerCase()
          if (platform.includes("mac")) return t("openInFinder")
          if (platform.includes("win")) return t("openInExplorer")
          return t("openInFileManager")
        })()

  const rootTarget: FileActionTarget = useMemo(
    () => ({ kind: "dir", path: "", name: rootNodeName }),
    [rootNodeName]
  )

  if (!folder) {
    return <AuxPanelNoFolderEmpty />
  }

  if (loading && nodes.length === 0) {
    return (
      <div className="p-3 space-y-2">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2 ml-4" />
        <Skeleton className="h-4 w-2/3 ml-4" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-3/4 ml-4" />
      </div>
    )
  }

  if (error) {
    return (
      <div className="p-3 text-xs text-destructive">
        <p>{error}</p>
        <Button
          variant="ghost"
          size="xs"
          className="mt-2"
          onClick={() => {
            void fetchTree()
          }}
        >
          {t("retry")}
        </Button>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {workspaceState.degraded && (
        <WorkspaceDegradedBanner onRetry={workspaceState.restart} />
      )}
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <ScrollArea className="flex-1 min-h-0 pb-1" x="scroll">
            <FileTree
              key={folder?.path ?? "file-tree-empty"}
              className="border-0 rounded-none bg-transparent w-max min-w-full"
              expanded={expandedPaths}
              onExpandedChange={setExpandedPaths}
              selectedPath={selectedTreePath}
              onSelect={handleTreeSelect}
            >
              {folder?.path && (
                <ContextMenu>
                  <ContextMenuTrigger>
                    <FileTreeFolder
                      path={FILE_TREE_ROOT_PATH}
                      name={rootNodeName}
                      className="font-medium"
                    >
                      {nodes.map((node) => (
                        <RenderNode
                          key={node.path}
                          node={node}
                          expandedPaths={expandedPaths}
                          workspacePath={folder.path}
                          activeSessionTabId={activeSessionTabId}
                          webMode={webMode}
                          folderUploadSupported={folderUploadSupported}
                          onOpenFilePreview={(path) => {
                            void openFilePreview(path)
                          }}
                          onOpenDirInTerminal={handleOpenDirInTerminal}
                          onRequestCreate={handleRequestCreate}
                          onRequestRename={handleRequestRename}
                          onRequestDelete={handleRequestDelete}
                          onRequestUpload={handleRequestUpload}
                          onRequestDownloadFile={(target) =>
                            void handleRequestDownloadFile(target)
                          }
                          onRequestDownloadDir={(target) =>
                            void handleRequestDownloadDir(target)
                          }
                          onRefresh={fetchTree}
                        />
                      ))}
                    </FileTreeFolder>
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <ContextMenuSub>
                      <ContextMenuSubTrigger>{t("new")}</ContextMenuSubTrigger>
                      <ContextMenuSubContent>
                        <ContextMenuItem
                          onSelect={() => handleRequestCreate("", "file")}
                        >
                          {t("newFile")}
                        </ContextMenuItem>
                        <ContextMenuItem
                          onSelect={() => handleRequestCreate("", "dir")}
                        >
                          {t("newDirectory")}
                        </ContextMenuItem>
                      </ContextMenuSubContent>
                    </ContextMenuSub>
                    <ContextMenuItem
                      onSelect={() => {
                        void fetchTree()
                      }}
                    >
                      {t("reloadFromDisk")}
                    </ContextMenuItem>
                    <ContextMenuSub>
                      <ContextMenuSubTrigger>
                        {t("openIn")}
                      </ContextMenuSubTrigger>
                      <ContextMenuSubContent>
                        <ContextMenuItem
                          onSelect={() => {
                            void revealItemInDir(folder.path)
                          }}
                        >
                          {systemExplorerLabel}
                        </ContextMenuItem>
                        <ContextMenuItem
                          onSelect={() => {
                            void handleOpenDirInTerminal(
                              folder.path,
                              rootNodeName
                            )
                          }}
                        >
                          {t("openInTerminal")}
                        </ContextMenuItem>
                      </ContextMenuSubContent>
                    </ContextMenuSub>
                    <ContextMenuItem
                      onSelect={() =>
                        void copyPathToClipboard(folder.path, {
                          success: t("toasts.pathCopied"),
                          failure: t("toasts.copyPathFailed"),
                        })
                      }
                    >
                      {t("copyPath")}
                    </ContextMenuItem>
                    {webMode && (
                      <>
                        <ContextMenuItem
                          onSelect={() => handleRequestUpload("")}
                        >
                          {t("upload")}
                        </ContextMenuItem>
                        <ContextMenuItem
                          onSelect={() =>
                            void handleRequestDownloadDir(rootTarget)
                          }
                        >
                          {t("downloadAsZip")}
                        </ContextMenuItem>
                      </>
                    )}
                  </ContextMenuContent>
                </ContextMenu>
              )}
            </FileTree>
          </ScrollArea>
        </ContextMenuTrigger>
        <ContextMenuContent>
          <ContextMenuSub>
            <ContextMenuSubTrigger>{t("new")}</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuItem onSelect={() => handleRequestCreate("", "file")}>
                {t("newFile")}
              </ContextMenuItem>
              <ContextMenuItem onSelect={() => handleRequestCreate("", "dir")}>
                {t("newDirectory")}
              </ContextMenuItem>
            </ContextMenuSubContent>
          </ContextMenuSub>
          {webMode && (
            <ContextMenuItem onSelect={() => handleRequestUpload("")}>
              {t("upload")}
            </ContextMenuItem>
          )}
          <ContextMenuItem
            onSelect={() => {
              void fetchTree()
            }}
          >
            {t("reloadFromDisk")}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {webMode && folder?.path && (
        <WorkspaceUploadDialog
          open={uploadDialogOpen}
          onOpenChange={setUploadDialogOpen}
          rootPath={folder.path}
          targetPath={uploadDialogTarget}
          folderUploadSupported={folderUploadSupported}
          onComplete={handleUploadComplete}
        />
      )}

      <Dialog
        open={createParentPath !== null}
        onOpenChange={(open) => {
          if (open) return
          setCreateParentPath(null)
          setCreateName("")
        }}
      >
        <DialogContent
          onOpenAutoFocus={(e) => {
            e.preventDefault()
            const input = (
              e.currentTarget as HTMLElement | null
            )?.querySelector("input")
            if (input) requestAnimationFrame(() => input.focus())
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {createKind === "dir"
                ? t("createDialog.newDirectory")
                : t("createDialog.newFile")}
            </DialogTitle>
            <DialogDescription>
              {t("createDialog.description", {
                kind:
                  createKind === "dir"
                    ? t("newDirectory").toLowerCase()
                    : t("newFile").toLowerCase(),
              })}
            </DialogDescription>
          </DialogHeader>
          <form
            onSubmit={(event) => {
              event.preventDefault()
              void handleCreateConfirm()
            }}
            className="space-y-4"
          >
            <Input
              value={createName}
              onChange={(event) => setCreateName(event.target.value)}
              disabled={creating}
              placeholder={
                createKind === "dir"
                  ? t("createDialog.placeholderDirectory")
                  : t("createDialog.placeholderFile")
              }
            />
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={creating}
                onClick={() => {
                  setCreateParentPath(null)
                  setCreateName("")
                }}
              >
                {tCommon("cancel")}
              </Button>
              <Button type="submit" disabled={creating || !createName.trim()}>
                {tCommon("create")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(renameTarget)}
        onOpenChange={(open) => {
          if (open) return
          setRenameTarget(null)
          setRenameValue("")
        }}
      >
        <DialogContent
          onOpenAutoFocus={(e) => {
            e.preventDefault()
            const input = (
              e.currentTarget as HTMLElement | null
            )?.querySelector("input")
            if (input) requestAnimationFrame(() => input.focus())
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {renameTarget?.kind === "dir"
                ? t("renameDialog.renameDirectory")
                : t("renameDialog.renameFile")}
            </DialogTitle>
            <DialogDescription>
              {t("renameDialog.description")}
            </DialogDescription>
          </DialogHeader>
          <form
            onSubmit={(event) => {
              event.preventDefault()
              void handleRenameConfirm()
            }}
            className="space-y-4"
          >
            <Input
              value={renameValue}
              onChange={(event) => setRenameValue(event.target.value)}
              disabled={renaming}
              placeholder={
                renameTarget?.kind === "dir"
                  ? t("renameDialog.placeholderDirectory")
                  : t("renameDialog.placeholderFile")
              }
            />
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={renaming}
                onClick={() => {
                  setRenameTarget(null)
                  setRenameValue("")
                }}
              >
                {tCommon("cancel")}
              </Button>
              <Button type="submit" disabled={renaming}>
                {tCommon("confirm")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={Boolean(deleteTarget)}
        onOpenChange={(open) => {
          if (open) return
          setDeleteTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteConfirm.title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {deleteTarget
                ? t("deleteConfirm.descriptionWithTarget", {
                    kind:
                      deleteTarget.kind === "dir"
                        ? t("deleteConfirm.kindDirectory")
                        : t("deleteConfirm.kindFile"),
                    name: deleteTarget.name,
                  })
                : t("deleteConfirm.descriptionFallback")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>
              {tCommon("cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={deleting}
              onClick={() => {
                void handleDeleteConfirm()
              }}
            >
              {tCommon("delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
