"use client"

import {
  ChevronLeft,
  ChevronRight,
  Loader2,
  RefreshCw,
  Search,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { UserMemoryDocumentId } from "@/lib/user-memory-documents"
import { UserMemoryEntryRow } from "./user-memory-entry-row"
import {
  MEMORY_ENTRY_PAGE_SIZE,
  forgetMemoryEntry,
  saveMemoryEntry,
  toggleMemoryEntry,
  useUserMemoryEntries,
} from "./use-user-memory-entries"

export function UserMemoryEntriesPanel({
  document,
  disabled,
  onUpdated,
}: {
  document: UserMemoryDocumentId
  disabled: boolean
  onUpdated: () => void
}) {
  const state = useUserMemoryEntries(document)
  const changed = async (action: Promise<boolean>) => {
    const success = await action
    if (success) onUpdated()
    return success
  }
  return (
    <div className="space-y-3 p-4">
      <EntryToolbar state={state} disabled={disabled} />
      {state.error && (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error}
        </p>
      )}
      <EntryListContent
        state={state}
        disabled={disabled}
        changed={changed}
        onUpdated={onUpdated}
      />
    </div>
  )
}

type EntryState = ReturnType<typeof useUserMemoryEntries>

function EntryListContent({
  state,
  disabled,
  changed,
  onUpdated,
}: {
  state: EntryState
  disabled: boolean
  changed: (action: Promise<boolean>) => Promise<boolean>
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.entries")
  if (state.loading)
    return (
      <div
        role="status"
        className="flex items-center gap-2 py-6 text-sm text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" />
        {t("loading")}
      </div>
    )
  if (!state.page) return null
  return (
    <>
      {state.page.entries.length === 0 ? (
        <p className="py-6 text-sm text-muted-foreground">
          {t(state.query ? "noMatches" : "empty")}
        </p>
      ) : (
        <ul>
          {state.page.entries.map((entry) => (
            <UserMemoryEntryRow
              key={`${entry.id}:${entry.sourceRevision}:${entry.active}`}
              entry={entry}
              disabled={disabled || state.busy || state.page!.readonly}
              onSave={(content) =>
                changed(saveMemoryEntry(state, entry, content))
              }
              onToggle={() => changed(toggleMemoryEntry(state, entry))}
              onForget={async (purgeBackups) => {
                const result = await forgetMemoryEntry(
                  state,
                  entry,
                  purgeBackups
                )
                if (result) onUpdated()
                return result
              }}
            />
          ))}
        </ul>
      )}
      <EntryPagination state={state} />
    </>
  )
}

function EntryToolbar({
  state,
  disabled,
}: {
  state: EntryState
  disabled: boolean
}) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <div className="flex flex-wrap items-center gap-3">
      <div className="relative min-w-40 flex-1">
        <Search className="pointer-events-none absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
        <Input
          aria-label={t("search")}
          placeholder={t("search")}
          value={state.query}
          maxLength={512}
          disabled={disabled || state.busy}
          onChange={(event) => state.setQuery(event.target.value)}
          className="pl-9"
        />
      </div>
      <label className="flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          checked={state.includeInactive}
          disabled={disabled || state.busy}
          onChange={(event) => state.setIncludeInactive(event.target.checked)}
        />
        {t("showInactive")}
      </label>
      <Button
        size="icon"
        variant="ghost"
        title={t("refresh")}
        aria-label={t("refresh")}
        disabled={state.loading || state.busy}
        onClick={state.reload}
      >
        <RefreshCw className="size-4" />
      </Button>
    </div>
  )
}

function EntryPagination({ state }: { state: EntryState }) {
  const t = useTranslations("UserMemorySettings.entries")
  if (!state.page) return null
  return (
    <div className="flex items-center justify-between gap-2 border-t pt-3 text-xs text-muted-foreground">
      <span>
        {state.page.total} {t("count")}
      </span>
      <div className="flex gap-1">
        <Button
          variant="ghost"
          size="icon"
          title={t("previous")}
          aria-label={t("previous")}
          disabled={state.busy || state.offset === 0}
          onClick={() =>
            state.setOffset(Math.max(0, state.offset - MEMORY_ENTRY_PAGE_SIZE))
          }
        >
          <ChevronLeft className="size-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title={t("next")}
          aria-label={t("next")}
          disabled={
            state.busy ||
            state.offset + MEMORY_ENTRY_PAGE_SIZE >= state.page.total
          }
          onClick={() => state.setOffset(state.offset + MEMORY_ENTRY_PAGE_SIZE)}
        >
          <ChevronRight className="size-4" />
        </Button>
      </div>
    </div>
  )
}
