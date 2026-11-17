"use client"

import { BookOpenText, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { useKnowledgeComposerSelection } from "./use-knowledge-composer-selection"

export function KnowledgeComposerChip({
  tabId,
  onOpen,
}: {
  tabId?: string | null
  onOpen: () => void
}) {
  const t = useTranslations("CloudKnowledge")
  const { selection, busy, clear } = useKnowledgeComposerSelection(tabId)
  if (!tabId || !selection?.selected.length) return null

  const label =
    selection.selected.length === 1
      ? selection.selected[0].name
      : t("chipLabel", { count: selection.selected.length })
  const title = selection.selected.map((item) => item.name).join(", ")
  return (
    <div className="px-3 pt-2" data-knowledge-composer-selection>
      <span className="inline-flex max-w-full items-center gap-1 rounded-md">
        <button
          type="button"
          title={title}
          aria-label={t("editChip", { name: title })}
          className="inline-flex max-w-full items-center gap-1.5 rounded-md border border-violet-500/25 bg-violet-500/10 px-2 py-1 text-xs font-medium text-violet-700 transition-colors hover:border-violet-500/40 hover:bg-violet-500/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/50 dark:text-violet-300"
          onClick={onOpen}
        >
          <BookOpenText className="size-3.5 shrink-0" />
          <span className="truncate">{label}</span>
        </button>
        <button
          type="button"
          className="flex min-h-7 min-w-7 items-center justify-center rounded-md hover:bg-muted max-sm:min-h-11 max-sm:min-w-11"
          aria-label={t("removeChip", { name: title })}
          disabled={busy}
          onClick={() => void clear()}
        >
          <X className="size-3.5" />
        </button>
      </span>
    </div>
  )
}
