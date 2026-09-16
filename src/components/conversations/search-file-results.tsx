"use client"

import { useState } from "react"
import { ChevronLeft, ChevronRight, File, Folder } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import type { FlatFileEntry } from "@/hooks/use-file-tree"
import type { TaskArtifactInfo } from "@/lib/api"
import { formatConversationTitle } from "@/lib/conversation-title"
import { TaskArtifactTypeIcon } from "@/components/layout/task-artifact-type-icon"
import { Button } from "@/components/ui/button"
import { CommandGroup, CommandItem } from "@/components/ui/command"
import { SearchRequestStatus } from "./search-conversation-results"
import {
  SEARCH_PAGE_SIZE,
  useArtifactSearch,
  useSearchFiles,
} from "./use-command-search"

interface FileSearchProps {
  query: string
  folderId: number | null
  folderPath: string | undefined
  onClose: () => void
  onSelectArtifact: (item: TaskArtifactInfo) => void
}

export function FileSearchResults(props: FileSearchProps) {
  return (
    <>
      <ArtifactSearchResults key={props.query.trim()} {...props} />
      {props.folderPath && <WorkspaceFileSearch {...props} />}
    </>
  )
}

function ArtifactSearchResults({
  query,
  folderId,
  onSelectArtifact,
}: FileSearchProps) {
  const t = useTranslations("Folder.taskArtifacts")
  const [page, setPage] = useState(1)
  const search = useArtifactSearch({ query, folderId, page })
  return (
    <CommandGroup heading={t("open")}>
      <SearchRequestStatus {...search} />
      {search.data?.items.map((item) => (
        <ArtifactSearchRow
          key={item.id}
          item={item}
          onSelect={() => onSelectArtifact(item)}
        />
      ))}
      {search.data?.total === 0 && (
        <div className="px-2 py-4 text-sm text-muted-foreground">
          {t("emptySearch")}
        </div>
      )}
      {search.data && search.data.total > search.data.pageSize && (
        <ArtifactSearchPagination
          page={search.data.page}
          totalPages={Math.ceil(search.data.total / search.data.pageSize)}
          onChange={setPage}
        />
      )}
    </CommandGroup>
  )
}

function ArtifactSearchRow({
  item,
  onSelect,
}: {
  item: TaskArtifactInfo
  onSelect: () => void
}) {
  const t = useTranslations("Folder.search")
  return (
    <CommandItem value={`artifact:${item.id}`} onSelect={onSelect}>
      <TaskArtifactTypeIcon item={item} />
      <span className="min-w-0 flex-1">
        <span className="block truncate">{item.displayName}</span>
        <span
          className="block truncate text-xs text-muted-foreground"
          title={item.path}
        >
          {item.path}
        </span>
      </span>
      <span className="max-w-32 truncate text-xs text-muted-foreground">
        {formatConversationTitle(item.conversationTitle) ||
          t("untitledConversation")}
      </span>
    </CommandItem>
  )
}

function ArtifactSearchPagination({
  page,
  totalPages,
  onChange,
}: {
  page: number
  totalPages: number
  onChange: (page: number) => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div className="flex items-center justify-end gap-2 px-2 py-2">
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={page <= 1}
        onClick={() => onChange(page - 1)}
        title={t("paginationPrevious")}
        aria-label={t("paginationPrevious")}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <span className="text-xs text-muted-foreground">
        {page} / {totalPages}
      </span>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={page >= totalPages}
        onClick={() => onChange(page + 1)}
        title={t("paginationNext")}
        aria-label={t("paginationNext")}
      >
        <ChevronRight className="size-4" />
      </Button>
    </div>
  )
}

function WorkspaceFileSearch({ query, folderPath, onClose }: FileSearchProps) {
  const t = useTranslations("Folder.search")
  const search = useSearchFiles(folderPath)
  const select = useSelectSearchFile(onClose)
  const lower = query.trim().toLowerCase()
  const files = (search.data ?? [])
    .filter(
      (file) => file.lowerName.includes(lower) || file.lowerPath.includes(lower)
    )
    .slice(0, SEARCH_PAGE_SIZE)
  return (
    <CommandGroup heading={t("projectFiles")}>
      <SearchRequestStatus {...search} />
      {files.map((entry) => (
        <WorkspaceFileRow
          key={entry.relativePath}
          entry={entry}
          onSelect={() => void select(entry)}
        />
      ))}
      {!search.loading && !search.error && files.length === 0 && (
        <div className="px-2 py-4 text-sm text-muted-foreground">
          {t("noResults")}
        </div>
      )}
    </CommandGroup>
  )
}

function WorkspaceFileRow({
  entry,
  onSelect,
}: {
  entry: FlatFileEntry
  onSelect: () => void
}) {
  return (
    <CommandItem value={`file:${entry.relativePath}`} onSelect={onSelect}>
      {entry.kind === "dir" ? (
        <Folder className="size-4 text-blue-500" />
      ) : (
        <File className="size-4 text-muted-foreground" />
      )}
      <span className="min-w-0 flex-1 truncate">{entry.name}</span>
      <span className="max-w-48 truncate text-xs text-muted-foreground">
        {entry.relativePath}
      </span>
    </CommandItem>
  )
}

function useSelectSearchFile(onClose: () => void) {
  const t = useTranslations("Folder.taskArtifacts")
  const { revealInFileTree } = useAuxPanelContext()
  const { openFilePreview } = useWorkspaceActions()
  const { openConversations } = useWorkbenchRoute()
  return async (entry: FlatFileEntry) => {
    try {
      openConversations()
      if (entry.kind === "dir") revealInFileTree(entry.relativePath)
      else {
        const lastSlash = entry.relativePath.lastIndexOf("/")
        if (lastSlash > 0)
          revealInFileTree(entry.relativePath.slice(0, lastSlash))
        await openFilePreview(entry.relativePath)
      }
      onClose()
    } catch (error) {
      console.error("[command-search] file open failed", {
        errorType: error instanceof Error ? error.name : typeof error,
      })
      toast.error(t("openWorkspaceFailed"))
    }
  }
}
