"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, RefreshCcw, X } from "lucide-react"
import type { InteractiveHtmlState } from "@/lib/types"
import { useHtmlCard, type HtmlCardState } from "./use-interactive-html"
import { MAX_ANSWER_CHARS } from "./use-ask-question"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { InteractiveHtmlFrame } from "./interactive-html-frame"
import { useInteractivePage } from "@/contexts/interactive-page-context"

export type HtmlCardProps = {
  page: InteractiveHtmlState
  connectionId?: string | null
  preview?: boolean
  panel?: boolean
  responseOnly?: boolean
}

export function InteractiveHtmlCard(props: HtmlCardProps) {
  const card = useHtmlCard(props)
  if (card.closed) return null
  return (
    <section
      aria-label={props.page.title}
      className={cn(
        "flex min-h-0 shrink-0 flex-col overflow-hidden rounded-xl border bg-background",
        props.panel ? "h-full rounded-none border-0" : "max-h-[70svh]"
      )}
    >
      <HtmlHeader
        title={props.page.title}
        card={card}
        responseOnly={props.responseOnly}
      />
      <HtmlError card={card} />
      {!props.responseOnly && (
        <InteractiveHtmlFrame
          key={card.revision}
          html={props.page.html}
          title={props.page.title}
          waiting={card.waiting}
          onReady={card.ready}
          onError={card.setError}
          onSubmit={(data) => card.respond("submit", data)}
        />
      )}
      {card.waiting && (
        <HtmlTextResponse
          busy={card.busy}
          submit={(text) => card.respond("text", text)}
        />
      )}
    </section>
  )
}

export function InteractiveHtmlPages({
  pages,
  connectionId,
  onOpenPage,
}: {
  pages: InteractiveHtmlState[]
  connectionId: string | null
  onOpenPage?: () => void
}) {
  const { openLivePage } = useInteractivePage()
  if (!connectionId || !pages.length) return null
  return (
    <div className="flex flex-wrap gap-2 p-2">
      {pages.map((page) => (
        <Button
          key={page.interaction_id}
          size="sm"
          variant="outline"
          onClick={() => {
            openLivePage(page, connectionId)
            onOpenPage?.()
          }}
        >
          {page.title}
        </Button>
      ))}
    </div>
  )
}

function HtmlTextResponse({
  busy,
  submit,
}: {
  busy: boolean
  submit: (text: string) => Promise<void>
}) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  const [text, setText] = useState("")
  return (
    <details className="shrink-0 border-t px-3 py-2 text-xs">
      <summary className="cursor-pointer text-muted-foreground">
        {t("textFallback")}
      </summary>
      <p className="mt-2 text-muted-foreground">{t("reloadNotice")}</p>
      <div className="mt-2 flex items-end gap-2">
        <textarea
          aria-label={t("textFallback")}
          value={text}
          onChange={(event) => setText(event.target.value)}
          disabled={busy}
          maxLength={MAX_ANSWER_CHARS}
          rows={2}
          className="min-w-0 flex-1 rounded-md border p-2 text-sm"
        />
        <Button
          size="sm"
          disabled={busy || !text.trim()}
          onClick={() => void submit(text.trim()).catch(() => {})}
        >
          {t("submit")}
        </Button>
      </div>
    </details>
  )
}

function HtmlHeader({
  title,
  card,
  responseOnly,
}: {
  title: string
  card: HtmlCardState
  responseOnly?: boolean
}) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  return (
    <div className="flex shrink-0 items-center gap-2 border-b px-3 py-2">
      <span className="min-w-0 flex-1 truncate text-sm font-medium">
        {title}
      </span>
      <span className="text-xs text-muted-foreground">
        {card.waiting ? t("waiting") : t("explore")}
      </span>
      {card.loading && (
        <Loader2 aria-label={t("loading")} className="size-4 animate-spin" />
      )}
      {!responseOnly && (
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t("reload")}
          onClick={card.retry}
          disabled={card.busy}
        >
          <RefreshCcw />
        </Button>
      )}
      <Button
        size="icon-sm"
        variant="ghost"
        disabled={card.busy}
        aria-label={t("close")}
        onClick={() => void card.respond("close").catch(() => {})}
      >
        <X />
      </Button>
    </div>
  )
}

function HtmlError({ card }: { card: HtmlCardState }) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  return (
    <>
      {card.error && (
        <div
          role="alert"
          className="flex items-center gap-2 px-3 py-2 text-xs text-destructive"
        >
          <span className="flex-1">{t(card.error)}</span>
          <Button
            size="sm"
            variant="outline"
            disabled={card.busy}
            onClick={card.retry}
          >
            <RefreshCcw className="size-3" />
            {t("reload")}
          </Button>
        </div>
      )}
    </>
  )
}
