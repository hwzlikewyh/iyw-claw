"use client"

import { useState } from "react"
import { ChevronDown, ChevronRight, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type {
  KnowledgeCategory,
  KnowledgeChoice,
  KnowledgeFile,
  KnowledgeFolder,
} from "@/lib/cloud-knowledge"
import { useKnowledgeFiles, useKnowledgeFolders } from "./use-knowledge-browse"
import { KnowledgeChoiceRow } from "./knowledge-choice-row"

export interface KnowledgeTreeProps {
  category: KnowledgeCategory
  selected: KnowledgeChoice[]
  onToggle: (choice: KnowledgeChoice) => void
  disabled: boolean
  query: string
}

type FolderProps = KnowledgeTreeProps & {
  folder: KnowledgeFolder
  inherited: boolean
}

export function KnowledgeTree(props: KnowledgeTreeProps) {
  const t = useTranslations("CloudKnowledge")
  const { folders, error, retry } = useKnowledgeFolders(props.category)
  if (error) return <KnowledgeLoadError error={error} retry={retry} />
  if (!folders) return <KnowledgeLoading />
  const visible = folders.filter((folder) => matchesFolder(folder, props.query))
  if (!visible.length)
    return (
      <p className="p-4 text-sm text-muted-foreground">
        {t(folders.length ? "noMatches" : "empty")}
      </p>
    )
  return (
    <div className="py-1">
      {visible.map((folder) => (
        <FolderRow
          key={folder.folderId}
          {...props}
          folder={folder}
          inherited={false}
        />
      ))}
    </div>
  )
}

function matchesFolder(folder: KnowledgeFolder, query: string): boolean {
  return (
    !query ||
    folder.folderName.toLocaleLowerCase().includes(query) ||
    folder.children.some((child) => matchesFolder(child, query))
  )
}

function folderIds(folder: KnowledgeFolder): string[] {
  return [folder.folderId, ...folder.children.flatMap(folderIds)]
}

function FolderRow(props: FolderProps) {
  const { folder, category, inherited, disabled, onToggle, query } = props
  const [expanded, setExpanded] = useState(false)
  const { chosen, partial } = folderSelection(props)
  const autoExpanded = Boolean(
    query && folder.children.some((child) => matchesFolder(child, query))
  )
  const open = expanded || autoExpanded
  const choice: KnowledgeChoice = {
    category,
    kind: "folder",
    id: folder.folderId,
    name: folder.folderName,
    folderId: folder.folderId,
  }
  return (
    <div>
      <KnowledgeChoiceRow
        choice={choice}
        checked={chosen || inherited ? true : partial ? "indeterminate" : false}
        disabled={disabled || inherited}
        onToggle={onToggle}
        status={folder.fileCount}
      >
        <FolderToggle
          name={folder.folderName}
          open={open}
          disabled={autoExpanded}
          onClick={() => setExpanded(!open)}
        />
      </KnowledgeChoiceRow>
      {open && (
        <div className="ml-5 border-l pl-2">
          {folder.children
            .filter((child) => matchesFolder(child, query))
            .map((child) => (
              <FolderRow
                key={child.folderId}
                {...props}
                folder={child}
                inherited={inherited || chosen}
              />
            ))}
          <FileRows {...props} inherited={inherited || chosen} />
        </div>
      )}
    </div>
  )
}

function folderSelection({ folder, selected, category }: FolderProps) {
  const chosen = selected.some(
    (item) =>
      item.category === category &&
      item.kind === "folder" &&
      item.id === folder.folderId
  )
  const descendants = new Set(folderIds(folder))
  const partial = selected.some(
    (item) => item.category === category && descendants.has(item.folderId)
  )
  return { chosen, partial }
}

function FolderToggle({
  name,
  open,
  disabled,
  onClick,
}: {
  name: string
  open: boolean
  disabled: boolean
  onClick: () => void
}) {
  const t = useTranslations("CloudKnowledge")
  return (
    <button
      type="button"
      className="flex size-9 shrink-0 items-center justify-center rounded-sm focus-visible:ring-2 focus-visible:ring-ring"
      aria-label={t(open ? "collapse" : "expand", { name })}
      aria-expanded={open}
      onClick={onClick}
      disabled={disabled}
    >
      {open ? (
        <ChevronDown className="size-4" />
      ) : (
        <ChevronRight className="size-4" />
      )}
    </button>
  )
}

function FileRows(props: FolderProps) {
  const state = useKnowledgeFiles(props.category, props.folder.folderId)
  return (
    <div>
      {state.files.map((file) => (
        <FileRow key={file.fileId} {...props} file={file} />
      ))}
      <FilePaging
        state={state}
        hasChildren={props.folder.children.length > 0}
      />
    </div>
  )
}

function FilePaging({
  state,
  hasChildren,
}: {
  state: ReturnType<typeof useKnowledgeFiles>
  hasChildren: boolean
}) {
  const t = useTranslations("CloudKnowledge")
  if (state.error)
    return (
      <KnowledgeLoadError error={state.error} retry={() => void state.load()} />
    )
  if (state.busy) return <KnowledgeLoading />
  if (state.files.length < state.total)
    return (
      <Button variant="ghost" size="sm" onClick={() => void state.load()}>
        {t("loadMore", { loaded: state.files.length, total: state.total })}
      </Button>
    )
  if (!state.total && !hasChildren)
    return (
      <p className="px-3 py-2 text-xs text-muted-foreground">
        {t("emptyFolder")}
      </p>
    )
  return null
}

function FileRow(
  props: KnowledgeTreeProps & { file: KnowledgeFile; inherited: boolean }
) {
  const { file, selected, category, inherited, onToggle, disabled } = props
  const t = useTranslations("CloudKnowledge")
  const chosen = selected.some(
    (item) =>
      item.category === category &&
      item.kind === "file" &&
      item.id === file.fileId
  )
  const ready = Boolean(file.docId)
  const choice: KnowledgeChoice = {
    category,
    kind: "file",
    id: file.fileId,
    name: file.fileName,
    folderId: file.folderId,
  }
  return (
    <KnowledgeChoiceRow
      choice={choice}
      checked={chosen || (inherited && ready)}
      disabled={disabled || inherited || !ready}
      onToggle={onToggle}
      status={ready ? null : t("notReady")}
    >
      <span className="w-9 shrink-0" />
    </KnowledgeChoiceRow>
  )
}

export function KnowledgeLoading() {
  const t = useTranslations("CloudKnowledge")
  return (
    <div
      role="status"
      className="flex items-center gap-2 p-4 text-sm text-muted-foreground"
    >
      <Loader2 className="size-4 animate-spin" />
      {t("loading")}
    </div>
  )
}

export function KnowledgeLoadError({
  error,
  retry,
}: {
  error: string
  retry: () => void
}) {
  const t = useTranslations("CloudKnowledge")
  return (
    <div className="p-3 text-sm">
      <p role="alert" className="break-words text-destructive">
        {error}
      </p>
      <Button variant="ghost" size="sm" onClick={retry}>
        {t("retry")}
      </Button>
    </div>
  )
}
