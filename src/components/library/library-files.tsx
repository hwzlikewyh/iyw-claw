"use client"

import { useEffect, useState } from "react"
import { ChevronLeft, ChevronRight, RefreshCw, Search } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { LIBRARY_PAGE_SIZE, type LibraryTarget } from "@/lib/knowledge-library"
import type { Transport } from "@/lib/transport"
import { LibraryFileList } from "./library-file-list"
import { LibraryError, LibraryLoading } from "./library-states"
import { useLibraryFiles } from "./use-library-files"
import type { LibraryFileAction } from "./library-file-menu"

interface LibraryFilesProps {
  transport: Transport
  target: LibraryTarget
  revision: number
  reload: () => void
  onAction: (
    action: LibraryFileAction,
    file: import("@/lib/knowledge-library").LibraryFile
  ) => void
}

export function LibraryFiles(props: LibraryFilesProps) {
  const [search, setSearch] = useState("")
  const [query, setQuery] = useState("")
  const [page, setPage] = useState(1)
  const t = useTranslations("Library")
  useEffect(() => {
    const timer = window.setTimeout(() => {
      setPage(1)
      setQuery(search.trim())
    }, 250)
    return () => window.clearTimeout(timer)
  }, [search])
  const files = useLibraryFiles({ ...props, page, searchText: query })
  return (
    <section
      className="flex min-h-0 min-w-0 flex-1 flex-col"
      aria-label={t("files")}
    >
      <FileSearchToolbar
        search={search}
        setSearch={setSearch}
        busy={files.busy}
        reload={props.reload}
      />
      <div
        className="min-h-0 flex-1 overflow-auto rounded-md border"
        aria-busy={files.busy}
      >
        {files.error ? (
          <LibraryError error={files.error} retry={props.reload} />
        ) : files.busy && !files.data.list.length ? (
          <LibraryLoading />
        ) : (
          <LibraryFileList
            files={files.data.list}
            searching={Boolean(query)}
            onAction={props.onAction}
          />
        )}
      </div>
      {files.pollError && (
        <p role="status" className="pt-2 text-xs text-muted-foreground">
          {t("statusRefreshFailed")} {files.pollError}
        </p>
      )}
      <FilePagination
        total={files.data.total}
        page={page}
        busy={files.busy}
        onPage={setPage}
      />
    </section>
  )
}

function FileSearchToolbar({
  search,
  setSearch,
  busy,
  reload,
}: {
  search: string
  setSearch: (value: string) => void
  busy: boolean
  reload: () => void
}) {
  const t = useTranslations("Library")
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 pb-4">
      <label className="relative w-full max-w-sm">
        <span className="sr-only">{t("searchFiles")}</span>
        <Search
          className="absolute top-2.5 left-3 size-4 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={t("searchFiles")}
          maxLength={200}
          className="h-9 pl-9"
        />
      </label>
      <Button variant="ghost" size="sm" onClick={reload} disabled={busy}>
        <RefreshCw className="size-3.5" />
        {t("refresh")}
      </Button>
    </div>
  )
}

function FilePagination({
  total,
  page,
  busy,
  onPage,
}: {
  total: number
  page: number
  busy: boolean
  onPage: (page: number) => void
}) {
  const t = useTranslations("Library")
  const from = total ? Math.min((page - 1) * LIBRARY_PAGE_SIZE + 1, total) : 0
  const to = Math.min(page * LIBRARY_PAGE_SIZE, total)
  return (
    <footer className="flex items-center justify-between gap-2 pt-3 text-xs text-muted-foreground">
      <span className="tabular-nums">
        {t("fileRange", { from, to, total })}
      </span>
      <nav aria-label={t("pagination")} className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("previousPage")}
          disabled={busy || page <= 1}
          onClick={() => onPage(page - 1)}
        >
          <ChevronLeft className="size-4" />
        </Button>
        <span className="px-1 tabular-nums">{t("page", { page })}</span>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("nextPage")}
          disabled={busy || page * LIBRARY_PAGE_SIZE >= total}
          onClick={() => onPage(page + 1)}
        >
          <ChevronRight className="size-4" />
        </Button>
      </nav>
    </footer>
  )
}
