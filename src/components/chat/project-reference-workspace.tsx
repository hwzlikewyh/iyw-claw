"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import { WorkspaceTreePane } from "@/components/message/workspace-file-tree"
import { useLazyWorkspaceTree } from "@/components/message/workspace-file-tree-data"
import { joinFsPath } from "@/lib/path-utils"
import type { FileTreeNode } from "@/lib/types"
import type { ProjectReferenceSelection } from "./project-reference-artifacts"

function findNode(nodes: FileTreeNode[], path: string): FileTreeNode | null {
  for (const node of nodes) {
    if (node.path === path) return node
    if (node.kind !== "dir") continue
    const match = findNode(node.children, path)
    if (match) return match
  }
  return null
}

interface WorkspaceReferencePickerProps {
  rootPath: string
  selected: ProjectReferenceSelection | null
  onSelect: (selection: ProjectReferenceSelection) => void
}

function useWorkspacePicker({
  rootPath,
  selected,
  onSelect,
}: WorkspaceReferencePickerProps) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const tree = useLazyWorkspaceTree(rootPath)
  const pathErrors = useMemo(
    () =>
      new Map(
        [...tree.pathErrors.keys()].map((path) => [path, t("loadError")])
      ),
    [t, tree.pathErrors]
  )
  const root = rootPath.replace(/[\\/]+$/, "")
  const selectedPath =
    selected &&
    (selected.path.startsWith(`${root}/`) ||
      selected.path.startsWith(`${root}\\`))
      ? selected.path.slice(root.length + 1)
      : undefined
  return {
    ...tree,
    rootPath,
    pathErrors,
    selectedPath,
    error: tree.error ? t("loadError") : null,
    onSelect: (path: string) => {
      const node = findNode(tree.nodes, path)
      if (node)
        onSelect({
          path: joinFsPath(rootPath, path),
          name: node.name,
          kind: node.kind,
        })
    },
  }
}

export function WorkspaceReferencePicker(props: WorkspaceReferencePickerProps) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const picker = useWorkspacePicker(props)
  if (!picker.loading && !picker.error && picker.nodes.length === 0) {
    return (
      <p className="px-3 py-6 text-center text-sm text-muted-foreground">
        {t("emptyWorkspace")}
      </p>
    )
  }
  return (
    <div className="flex h-64 min-h-0 overflow-hidden rounded-md border [&>section]:w-full [&>section]:border-0">
      <WorkspaceTreePane {...picker} />
    </div>
  )
}
