"use client"

import { useDeferredValue, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, RefreshCw, Search } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { DbConversationSummary, MessageTurn } from "@/lib/types"
import { formatConversationTitle } from "@/lib/conversation-title"
import {
  conversationTurnText,
  loadConversationMenuHistory,
} from "./conversation-menu-data"
import { ConversationSearchPagination } from "./conversation-search-pagination"

const RESULTS_PER_PAGE = 20
const SNIPPET_CONTEXT = 180

function useMessageHistory(id: number) {
  const [attempt, setAttempt] = useState(0)
  const [result, setResult] = useState<{
    attempt: number
    turns: MessageTurn[]
    error: boolean
  } | null>(null)
  useEffect(() => {
    const controller = new AbortController()
    void loadConversationMenuHistory(id, controller.signal).then(
      (detail) => {
        if (!controller.signal.aborted)
          setResult({ attempt, turns: detail.turns, error: false })
      },
      (error) => {
        if (controller.signal.aborted) return
        console.error("[conversation-menu] search history failed", {
          conversationId: id,
          error,
        })
        setResult({ attempt, turns: [], error: true })
      }
    )
    return () => controller.abort()
  }, [id, attempt])
  return {
    result: result?.attempt === attempt ? result : null,
    retry: () => setAttempt((value) => value + 1),
  }
}

interface MessageSearchDialogProps {
  summary: DbConversationSummary
  onOpenChange: (open: boolean) => void
}

export function ConversationMessageSearchDialog(
  props: MessageSearchDialogProps
) {
  const t = useTranslations("Folder.conversationMenu")
  const history = useMessageHistory(props.summary.id)
  const [query, setQuery] = useState("")
  const [page, setPage] = useState(1)
  const deferredQuery = useDeferredValue(query.trim())
  return (
    <Dialog open onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-xl gap-4 rounded-lg p-5">
        <DialogHeader>
          <DialogTitle>{t("searchMessages")}</DialogTitle>
          <DialogDescription className="truncate">
            {formatConversationTitle(props.summary.title) || t("untitled")}
          </DialogDescription>
        </DialogHeader>
        <MessageQuery
          value={query}
          onChange={(value) => {
            setQuery(value)
            setPage(1)
          }}
        />
        {!history.result || history.result.error ? (
          <HistoryStatus
            error={history.result?.error ?? false}
            retry={history.retry}
          />
        ) : (
          <MessageMatches
            turns={history.result.turns}
            query={deferredQuery}
            page={page}
            onPageChange={setPage}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function MessageQuery(props: {
  value: string
  onChange: (value: string) => void
}) {
  const t = useTranslations("Folder.conversationMenu")
  return (
    <div className="relative">
      <Search
        aria-hidden
        className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
      />
      <Input
        autoFocus
        aria-label={t("searchMessages")}
        placeholder={t("searchPlaceholder")}
        className="pl-9"
        value={props.value}
        onChange={(event) => props.onChange(event.target.value)}
      />
    </div>
  )
}

function HistoryStatus({
  error,
  retry,
}: {
  error: boolean
  retry: () => void
}) {
  const t = useTranslations("Folder.conversationMenu")
  if (!error)
    return (
      <p
        role="status"
        className="flex items-center gap-2 text-sm text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" />
        {t("loading")}
      </p>
    )
  return (
    <div
      role="alert"
      className="flex items-center justify-between gap-3 text-sm text-destructive"
    >
      {t("loadFailed")}
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={retry}
        title={t("retry")}
        aria-label={t("retry")}
      >
        <RefreshCw className="size-4" />
      </Button>
    </div>
  )
}

function useMatches(turns: MessageTurn[], query: string) {
  const rows = useMemo(
    () =>
      turns
        .map((turn) => ({ turn, text: conversationTurnText(turn) }))
        .filter((row) => row.text.trim()),
    [turns]
  )
  return useMemo(
    () =>
      rows.filter((row) =>
        row.text.toLowerCase().includes(query.toLowerCase())
      ),
    [rows, query]
  )
}

function MessageMatches(props: {
  turns: MessageTurn[]
  query: string
  page: number
  onPageChange: (page: number) => void
}) {
  const t = useTranslations("Folder.conversationMenu")
  const matches = useMatches(props.turns, props.query)
  const page = Math.min(
    props.page,
    Math.max(1, Math.ceil(matches.length / RESULTS_PER_PAGE))
  )
  const visible = matches.slice(
    (page - 1) * RESULTS_PER_PAGE,
    page * RESULTS_PER_PAGE
  )
  return (
    <>
      <p role="status" className="text-xs text-muted-foreground">
        {matches.length
          ? t("matchCount", { count: matches.length })
          : t("noMatches")}
      </p>
      <div
        className="max-h-[50dvh] overflow-y-auto"
        key={`${props.query}:${page}`}
      >
        {visible.map(({ turn, text }, index) => (
          <MessageMatch
            key={`${turn.id}:${index}`}
            role={turn.role}
            text={text}
            query={props.query}
          />
        ))}
      </div>
      <ConversationSearchPagination
        page={page}
        loading={false}
        hasNext={page * RESULTS_PER_PAGE < matches.length}
        previous={() => props.onPageChange(page - 1)}
        next={() => props.onPageChange(page + 1)}
      />
    </>
  )
}

function MessageMatch(props: {
  role: MessageTurn["role"]
  text: string
  query: string
}) {
  const roles = useTranslations("Folder.conversation.exportLabels")
  return (
    <article className="border-b border-border/60 py-3 first:pt-0 last:border-0">
      <p className="mb-1 text-xs text-muted-foreground">{roles(props.role)}</p>
      <p className="whitespace-pre-wrap break-words text-sm leading-relaxed">
        <HighlightedSnippet text={props.text} query={props.query} />
      </p>
    </article>
  )
}

function HighlightedSnippet({ text, query }: { text: string; query: string }) {
  const firstMatch = query ? text.toLowerCase().indexOf(query.toLowerCase()) : 0
  const start = Math.max(0, firstMatch - SNIPPET_CONTEXT)
  const end = Math.min(text.length, firstMatch + query.length + SNIPPET_CONTEXT)
  const snippet = text.slice(start, end)
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
  const parts = query
    ? snippet.split(new RegExp(`(${escaped})`, "gi"))
    : [snippet]
  return (
    <>
      {start > 0 && "..."}
      {parts.map((part, index) =>
        index % 2 ? (
          <mark
            key={index}
            className="rounded-sm bg-amber-200 text-neutral-950"
          >
            {part}
          </mark>
        ) : (
          part
        )
      )}
      {end < text.length && "..."}
    </>
  )
}
