"use client"

import { useId, useState } from "react"
import { useTranslations } from "next-intl"
import { ControlField } from "./control-field"
import { Button } from "@/components/ui/button"
import { copyTextToClipboard } from "@/lib/utils"
import { flushUiStates } from "@/lib/intelligent-ui/state-store"
import {
  readAction,
  type ResolvedNode,
} from "@/lib/intelligent-ui/engine/block"
import { useUiRender, text } from "./context"
import { useUiScope } from "./scope"

export function Ask({ suggestion }: { suggestion: string }) {
  const t = useTranslations("IntelligentUI")
  const scope = useUiScope()
  const [draft, setDraft] = useDraft(suggestion)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState(false)
  async function send() {
    if (!scope?.ask || !draft.trim()) return
    setSending(true)
    setError(false)
    try {
      if (scope.persistedConversationId !== null)
        await flushUiStates(scope.persistedConversationId)
      await scope.ask(draft.trim())
    } catch {
      setError(true)
    } finally {
      setSending(false)
    }
  }
  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        <textarea
          aria-label={t("ask")}
          className="min-h-16 min-w-0 flex-1 rounded-md border bg-background p-2"
          value={draft}
          maxLength={2000}
          onChange={(event) => {
            setDraft(event.target.value)
          }}
        />
        <Button
          onClick={() => void send()}
          disabled={sending || scope?.streaming || !scope?.ask || !draft.trim()}
        >
          {t("send")}
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {t("stateSaveFailed")}
        </p>
      )}
    </div>
  )
}

export function Control({ node }: { node: ResolvedNode }) {
  const { state, stateNames } = useUiRender()
  const t = useTranslations("IntelligentUI")
  const p = node.props,
    bind = node.bind ?? "",
    value = state[bind]
  const id = useId()
  if (node.component === "button") return <ActionButton node={node} />
  if (!bind || !stateNames.has(bind)) return null
  return (
    <div className="min-w-0 space-y-1.5">
      <label
        htmlFor={id}
        className="flex items-center justify-between gap-2 text-xs text-muted-foreground"
      >
        <span>{text(p.label ?? bind)}</span>
        {node.component === "slider" && (
          <span>
            {text(value)} {text(p.unit ?? "")}
          </span>
        )}
      </label>
      <ControlField node={node} value={value} id={id} />
      <span className="sr-only">{t("input")}</span>
    </div>
  )
}

function ActionButton({ node }: { node: ResolvedNode }) {
  const { stateNames, update } = useUiRender()
  const t = useTranslations("IntelligentUI")
  const action = readAction(node.props, stateNames)
  if (action?.kind === "ask") return <Ask suggestion={action.text} />
  return (
    <Button
      size="sm"
      variant="outline"
      disabled={!action}
      onClick={() => {
        if (action?.kind === "set") update(action.patch)
        if (action?.kind === "copy") void copyTextToClipboard(action.text)
      }}
    >
      {text(node.props.label ?? t("copy"))}
    </Button>
  )
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
