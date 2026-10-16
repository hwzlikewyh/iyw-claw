"use client"

import { useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { toErrorMessage } from "@/lib/app-error"
import { flushUiStates } from "@/lib/intelligent-ui/state-store"
import { useUiScope } from "./scope"

type AskError = { stage: "stateSaveFailed" | "sendFailed"; detail: string }
type AskUnavailable = "replyInProgress" | "askUnavailable" | null

export function Ask({ suggestion }: { suggestion: string }) {
  const t = useTranslations("IntelligentUI")
  const [draft, setDraft] = useDraft(suggestion)
  const { send, sending, error, unavailable } = useAskAction(draft)
  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        <textarea
          aria-label={t("ask")}
          className="min-h-16 min-w-0 flex-1 rounded-md border bg-background p-2"
          value={draft}
          maxLength={2000}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button
          type="button"
          onClick={() => void send()}
          aria-busy={sending}
          disabled={sending || Boolean(unavailable) || !draft.trim()}
        >
          {t("send")}
        </Button>
      </div>
      {unavailable && (
        <p role="status" className="text-xs text-muted-foreground">
          {t(unavailable)}
        </p>
      )}
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {t(error.stage)} {error.detail}
        </p>
      )}
    </div>
  )
}

function useAskAction(draft: string) {
  const scope = useUiScope()
  const busy = useRef(false)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState<AskError | null>(null)
  const unavailable: AskUnavailable = scope?.streaming
    ? "replyInProgress"
    : !scope?.ask
      ? "askUnavailable"
      : null
  async function send() {
    if (!scope?.ask || unavailable || !draft.trim() || busy.current) return
    busy.current = true
    setSending(true)
    setError(null)
    let stage: AskError["stage"] = "stateSaveFailed"
    try {
      if (scope.persistedConversationId !== null)
        await flushUiStates(scope.persistedConversationId)
      stage = "sendFailed"
      await scope.ask(draft.trim())
    } catch (cause) {
      const detail = toErrorMessage(cause)
      console.error("[intelligent-ui] follow-up failed", {
        stage,
        conversationId: scope.conversationId,
        error: detail,
      })
      setError({ stage, detail })
    } finally {
      busy.current = false
      setSending(false)
    }
  }
  return { send, sending, error, unavailable }
}

function useDraft(suggestion: string): [string, (text: string) => void] {
  const [draft, setDraft] = useState(suggestion),
    [previous, setPrevious] = useState(suggestion),
    [edited, setEdited] = useState(false)
  if (previous !== suggestion) {
    setPrevious(suggestion)
    if (!edited) setDraft(suggestion)
  }
  return [
    draft,
    (text) => {
      setEdited(true)
      setDraft(text)
    },
  ]
}
