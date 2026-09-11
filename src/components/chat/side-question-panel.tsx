"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, X } from "lucide-react"
import type { useSideQuestion } from "@/hooks/use-side-question"
import { Button } from "@/components/ui/button"

export function SideQuestionPanel({
  side,
  onPromote,
  promoteDisabled = false,
}: {
  side: ReturnType<typeof useSideQuestion>
  onPromote: (text: string) => void
  promoteDisabled?: boolean
}) {
  const t = useTranslations("SideQuestion")
  const [draft, setDraft] = useState("")
  if (!side.open) return null
  return (
    <aside
      aria-label={t("title")}
      className="flex h-full min-h-0 w-80 max-w-[85vw] shrink-0 flex-col border-l bg-background"
    >
      <header className="flex items-center justify-between border-b px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold">
            {t("title")} <span className="text-muted-foreground">/btw</span>
          </h2>
          <p className="text-xs text-muted-foreground">{t("independent")}</p>
        </div>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={t("close")}
          onClick={() => side.setOpen(false)}
        >
          <X className="h-4 w-4" />
        </Button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <p className="mb-4 text-xs text-muted-foreground">{t("context")}</p>
        {!side.capability?.supported && (
          <p role="status" className="text-sm text-muted-foreground">
            {t("unavailable")}
          </p>
        )}
        {side.error && (
          <p
            role="alert"
            className="mb-3 whitespace-pre-wrap break-words text-xs text-destructive"
          >
            {side.error}
          </p>
        )}
        {side.entries.map((entry) => (
          <section key={entry.id} className="mb-5 border-b pb-4">
            <p className="rounded-lg bg-muted p-3 text-sm whitespace-pre-wrap break-words">
              {entry.question}
            </p>
            <div
              className="mt-3 whitespace-pre-wrap break-words text-sm"
              aria-live="polite"
            >
              {entry.status === "running" && (
                <span className="flex items-center gap-2">
                  <Loader2 className="h-3 w-3 animate-spin" />
                  {t("running")}
                </span>
              )}
              {entry.status === "cancelled" && (
                <span className="text-muted-foreground">{t("cancelled")}</span>
              )}
              {entry.status === "failed" && (
                <span className="text-destructive">
                  {entry.error ?? t("failed")}
                </span>
              )}
              {entry.answer}
            </div>
            {entry.answer && (
              <Button
                variant="ghost"
                size="sm"
                className="mt-2"
                disabled={promoteDisabled}
                title={promoteDisabled ? t("promoteEditing") : undefined}
                onClick={() =>
                  onPromote(
                    t("quote", {
                      question: entry.question,
                      answer: entry.answer!,
                    })
                  )
                }
              >
                {t("promote")}
              </Button>
            )}
          </section>
        ))}
      </div>
      <form
        className="border-t p-3"
        onSubmit={(event) => {
          event.preventDefault()
          if (draft.trim() && side.ask(draft.trim())) setDraft("")
        }}
      >
        <textarea
          aria-label={t("question")}
          placeholder={t("placeholder")}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          className="min-h-20 w-full resize-y rounded-md border bg-background p-2 text-sm"
        />
        <div className="mt-2 flex items-center justify-between gap-2">
          <span className="text-[11px] text-muted-foreground">
            {t("temporary")}
          </span>
          {side.running ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => void side.cancel()}
            >
              {t("stop")}
            </Button>
          ) : (
            <Button
              type="submit"
              size="sm"
              disabled={!side.capability?.supported || !draft.trim()}
            >
              {t("ask")}
            </Button>
          )}
        </div>
      </form>
    </aside>
  )
}
