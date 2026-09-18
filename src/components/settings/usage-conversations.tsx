"use client"

import { useState } from "react"
import { ChevronLeft, ChevronRight, Loader2 } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { SessionDetailsDialog } from "@/components/conversations/session-details-dialog"
import { UsagePointsValue } from "@/components/message/usage-points"
import { formatConversationTitle } from "@/lib/conversation-title"
import { formatTokenThousands } from "@/lib/token-format"
import {
  CONVERSATIONS_PAGE_SIZE,
  useUsageConversationList,
  useUsageConversationPage,
  type ConversationUsageRow,
} from "./use-usage-conversations"

function ConversationRow({
  row,
  onSelect,
}: {
  row: ConversationUsageRow
  onSelect: () => void
}) {
  const t = useTranslations("UsageSettings.conversations")
  const locale = useLocale()
  return (
    <tr className="border-b border-border/60 hover:bg-muted/25">
      <td className="py-3 pe-3">
        <ConversationTitle row={row} onSelect={onSelect} />
        {row.error && (
          <span className="text-destructive">{t("loadFailed")}</span>
        )}
      </td>
      <td
        className="hidden truncate px-2 py-3 text-muted-foreground @2xl:table-cell"
        title={row.summary.model ?? undefined}
      >
        {row.summary.model ?? "--"}
      </td>
      <ConversationUsageCells row={row} />
      <td className="hidden py-3 ps-2 text-right tabular-nums text-muted-foreground @lg:table-cell">
        {new Date(row.summary.updated_at).toLocaleDateString(locale, {
          month: "2-digit",
          day: "2-digit",
        })}
      </td>
    </tr>
  )
}

function ConversationTitle({
  row,
  onSelect,
}: {
  row: ConversationUsageRow
  onSelect: () => void
}) {
  const t = useTranslations("UsageSettings.conversations")
  const title = formatConversationTitle(row.summary.title) || t("untitled")
  return (
    <button
      type="button"
      onClick={onSelect}
      title={title}
      className="block w-full truncate rounded text-left font-medium hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {title}
    </button>
  )
}

function ConversationUsageCells({ row }: { row: ConversationUsageRow }) {
  const locale = useLocale()
  const usage = row.stats?.total_usage
  const total =
    row.stats?.total_tokens ??
    (usage
      ? usage.input_tokens +
        usage.output_tokens +
        usage.cache_read_input_tokens +
        usage.cache_creation_input_tokens
      : null)
  return (
    <>
      <td
        className="break-all px-2 py-3 text-right tabular-nums"
        title={
          total == null ? undefined : `${total.toLocaleString(locale)} Token`
        }
      >
        {total == null ? "--" : formatTokenThousands(total, locale)}
      </td>
      <td className="break-words px-2 py-3 text-right">
        <UsagePointsValue points={usage?.estimated_points} />
      </td>
    </>
  )
}

const CONVERSATION_COLUMNS = [
  { key: "conversation", className: "w-[40%] py-2 pe-3 text-left" },
  { key: "model", className: "hidden px-2 py-2 text-left @2xl:table-cell" },
  { key: "tokens", className: "px-2 py-2 text-right" },
  { key: "points", className: "px-2 py-2 text-right" },
  { key: "updatedAt", className: "hidden py-2 ps-2 text-right @lg:table-cell" },
] as const

function ConversationsTable({
  rows,
  onSelect,
}: {
  rows: ConversationUsageRow[]
  onSelect: (row: ConversationUsageRow) => void
}) {
  const t = useTranslations("UsageSettings.conversations")
  return (
    <table className="w-full table-fixed text-xs">
      <thead className="border-b text-muted-foreground">
        <tr>
          {CONVERSATION_COLUMNS.map(({ key, className }) => (
            <th key={key} scope="col" className={`${className} font-normal`}>
              {t(key)}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <ConversationRow
            key={row.summary.id}
            row={row}
            onSelect={() => onSelect(row)}
          />
        ))}
      </tbody>
    </table>
  )
}

function ConversationPagination({
  page,
  total,
  onPage,
}: {
  page: number
  total: number
  onPage: (page: number) => void
}) {
  const t = useTranslations("UsageSettings.conversations")
  const pages = Math.ceil(total / CONVERSATIONS_PAGE_SIZE)
  if (pages <= 1) return null
  return (
    <div className="flex items-center justify-end gap-2 text-xs text-muted-foreground">
      <span>{t("page", { page: page + 1, pages })}</span>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={page === 0}
        onClick={() => onPage(page - 1)}
        title={t("previous")}
        aria-label={t("previous")}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={page + 1 >= pages}
        onClick={() => onPage(page + 1)}
        title={t("next")}
        aria-label={t("next")}
      >
        <ChevronRight className="size-4" />
      </Button>
    </div>
  )
}

export function UsageConversations({ days }: { days: number }) {
  const t = useTranslations("UsageSettings")
  const list = useUsageConversationList(days)
  const [page, setPage] = useState(0)
  const rows = useUsageConversationPage(list.rows, page)
  const [selected, setSelected] = useState<ConversationUsageRow | null>(null)
  const loading = list.loading || rows === null
  return (
    <section className="min-w-0 space-y-3 border-t pt-5" aria-busy={loading}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-sm font-semibold">{t("conversations.title")}</h2>
        {!list.loading && !list.error && (
          <span className="text-xs text-muted-foreground">
            {t("conversations.count", { count: list.rows.length })}
          </span>
        )}
      </div>
      <ConversationsContent
        rows={rows}
        loading={loading}
        error={list.error}
        onSelect={setSelected}
      />
      <ConversationPagination
        page={page}
        total={list.rows.length}
        onPage={setPage}
      />
      {selected && (
        <SessionDetailsDialog
          open
          onOpenChange={(open) => !open && setSelected(null)}
          summary={selected.summary}
          stats={selected.stats}
        />
      )}
    </section>
  )
}

function ConversationsContent({
  rows,
  loading,
  error,
  onSelect,
}: {
  rows: ConversationUsageRow[] | null
  loading: boolean
  error: string | null
  onSelect: (row: ConversationUsageRow) => void
}) {
  const t = useTranslations("UsageSettings")
  if (error)
    return (
      <p role="alert" className="text-xs text-destructive">
        {t("loadFailed", { message: error })}
      </p>
    )
  if (loading || rows === null)
    return (
      <div
        role="status"
        className="flex min-h-24 items-center justify-center gap-2 text-xs text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" />
        {t("loading")}
      </div>
    )
  if (rows.length === 0)
    return (
      <p className="py-5 text-xs text-muted-foreground">
        {t("conversations.empty")}
      </p>
    )
  return <ConversationsTable rows={rows} onSelect={onSelect} />
}
