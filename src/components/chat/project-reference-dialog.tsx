"use client"

import { useCallback, useMemo, useState } from "react"
import { AlertCircle, Folder, PackageOpen } from "lucide-react"
import { useTranslations } from "next-intl"

import { WorkspaceTreePane } from "@/components/message/workspace-file-tree"
import { useLazyWorkspaceTree } from "@/components/message/workspace-file-tree-data"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { joinFsPath } from "@/lib/path-utils"
import type { FileTreeNode } from "@/lib/types"
import { cn } from "@/lib/utils"
import {
  ArtifactReferencePicker,
  useReferenceArtifacts,
  type ReferenceArtifactsState,
  type ProjectReferenceSelection,
} from "./project-reference-artifacts"
import { ProjectReferenceFooter } from "./project-reference-footer"

export type { ProjectReferenceSelection } from "./project-reference-artifacts"

interface ProjectReferenceDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  rootPath: string | null
  onSelect: (selection: ProjectReferenceSelection) => void
  onBrowseFolder: () => void | Promise<void>
}

interface ReferencePickerProps {
  selected: ProjectReferenceSelection | null
  onSelect: (selection: ProjectReferenceSelection) => void
}

function pathName(path: string): string {
  return (
    path
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() || path
  )
}

function isPathWithinRoot(path: string, rootPath: string): boolean {
  if (path === rootPath) return true
  const normalizedRoot = rootPath.replace(/[\\/]+$/, "")
  return (
    path.startsWith(`${normalizedRoot}/`) ||
    path.startsWith(`${normalizedRoot}\\`)
  )
}

function findNode(nodes: FileTreeNode[], path: string): FileTreeNode | null {
  for (const node of nodes) {
    if (node.path === path) return node
    if (node.kind === "dir") {
      const match = findNode(node.children, path)
      if (match) return match
    }
  }
  return null
}

function WorkspaceReferencePicker({
  rootPath,
  selected,
  onSelect,
}: ReferencePickerProps & { rootPath: string }) {
  const picker = useWorkspacePicker(rootPath, selected, onSelect)
  return (
    <div className="flex h-full min-h-0 flex-col">
      <WorkspaceRootRow
        rootPath={rootPath}
        selected={picker.wholeWorkspaceSelected}
        onSelect={onSelect}
      />
      <div className="flex min-h-0 flex-1">
        <WorkspaceTreePane
          {...picker.tree}
          error={picker.error}
          pathErrors={picker.pathErrors}
          rootPath={rootPath}
          selectedPath={picker.selectedPath}
          onSelect={picker.selectPath}
        />
      </div>
    </div>
  )
}

function useWorkspacePicker(
  rootPath: string,
  selected: ProjectReferenceSelection | null,
  onSelect: ReferencePickerProps["onSelect"]
) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const tree = useLazyWorkspaceTree(rootPath)
  const pathErrors = useMemo(
    () =>
      new Map(
        [...tree.pathErrors.keys()].map((path) => [path, t("loadError")])
      ),
    [t, tree.pathErrors]
  )
  const wholeWorkspaceSelected = selected?.path === rootPath
  const selectedPath =
    selected && isPathWithinRoot(selected.path, rootPath)
      ? selected.path.slice(rootPath.length).replace(/^[\\/]/, "")
      : undefined
  const selectPath = useCallback(
    (relativePath: string) => {
      const node = findNode(tree.nodes, relativePath)
      if (!node) return
      onSelect({
        path: joinFsPath(rootPath, relativePath),
        name: node.name,
        kind: node.kind === "dir" ? "dir" : "file",
      })
    },
    [onSelect, rootPath, tree.nodes]
  )

  return {
    tree,
    error: tree.error ? t("loadError") : null,
    pathErrors,
    wholeWorkspaceSelected,
    selectedPath,
    selectPath,
  }
}

function WorkspaceRootRow({
  rootPath,
  selected,
  onSelect,
}: {
  rootPath: string
  selected: boolean
  onSelect: ReferencePickerProps["onSelect"]
}) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  return (
    <button
      type="button"
      onClick={() =>
        onSelect({ path: rootPath, name: pathName(rootPath), kind: "dir" })
      }
      className={cn(
        "flex h-10 shrink-0 items-center gap-2 border-b px-3 text-left text-sm hover:bg-muted/50",
        selected && "bg-muted"
      )}
    >
      <Folder className="size-4 text-blue-500" />
      <span className="min-w-0 flex-1 truncate">{t("wholeWorkspace")}</span>
    </button>
  )
}

function ReferenceState({
  icon: Icon,
  text,
}: {
  icon: typeof AlertCircle
  text: string
}) {
  return (
    <div className="flex min-h-48 items-center justify-center gap-2 text-sm text-muted-foreground">
      <Icon className="size-4" />
      {text}
    </div>
  )
}

export function ProjectReferenceDialog({
  open,
  onOpenChange,
  rootPath,
  onSelect,
  onBrowseFolder,
}: ProjectReferenceDialogProps) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const { activeFolderId } = useActiveFolder()
  const artifacts = useReferenceArtifacts(open, activeFolderId)
  const [selected, setSelected] = useState<ProjectReferenceSelection | null>(
    null
  )

  const changeOpen = (next: boolean) => {
    if (!next) setSelected(null)
    onOpenChange(next)
  }
  const confirm = () => {
    if (!selected) return
    onSelect(selected)
    changeOpen(false)
  }
  const browseFolder = () => {
    changeOpen(false)
    void onBrowseFolder()
  }

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent className="grid h-[min(42rem,calc(100dvh-2rem))] max-w-[min(54rem,calc(100vw-2rem))] grid-rows-[auto_minmax(0,1fr)_auto] gap-3 overflow-hidden rounded-lg p-4 sm:max-w-[min(54rem,calc(100vw-2rem))]">
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription>{t("description")}</DialogDescription>
        </DialogHeader>
        <ReferenceTabs
          rootPath={rootPath}
          artifacts={artifacts}
          selected={selected}
          onSelect={setSelected}
        />
        <ProjectReferenceFooter
          selected={selected}
          onBrowseFolder={browseFolder}
          onConfirm={confirm}
        />
      </DialogContent>
    </Dialog>
  )
}

function ReferenceTabs({
  rootPath,
  artifacts,
  selected,
  onSelect,
}: ReferencePickerProps & {
  rootPath: string | null
  artifacts: ReferenceArtifactsState
}) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  return (
    <Tabs
      defaultValue={rootPath ? "workspace" : "artifacts"}
      className="min-h-0"
    >
      <TabsList className="h-9 w-full rounded-md">
        <TabsTrigger
          value="workspace"
          disabled={!rootPath}
          className="rounded-sm"
        >
          <Folder className="size-4" />
          {t("workspace")}
        </TabsTrigger>
        <TabsTrigger value="artifacts" className="rounded-sm">
          <PackageOpen className="size-4" />
          {t("artifacts")}
        </TabsTrigger>
      </TabsList>
      <TabsContent
        value="workspace"
        className="min-h-0 overflow-hidden rounded-md border"
      >
        {rootPath ? (
          <WorkspaceReferencePicker
            rootPath={rootPath}
            selected={selected}
            onSelect={onSelect}
          />
        ) : (
          <ReferenceState icon={AlertCircle} text={t("noWorkspace")} />
        )}
      </TabsContent>
      <TabsContent
        value="artifacts"
        className="min-h-0 overflow-hidden rounded-md border"
      >
        <ArtifactReferencePicker
          {...artifacts}
          selected={selected}
          onSelect={onSelect}
        />
      </TabsContent>
    </Tabs>
  )
}
