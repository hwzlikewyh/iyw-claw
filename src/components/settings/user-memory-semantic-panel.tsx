"use client"

import { useState } from "react"
import { Loader2, Search } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toErrorMessage } from "@/lib/app-error"
import { previewMemorySemantic } from "@/lib/user-memory-entries"

const MAX_QUERY_LENGTH = 512

function useSemanticSearch() {
  const [query, setQuery] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [items, setItems] = useState<
    Awaited<ReturnType<typeof previewMemorySemantic>>["items"] | null
  >(null)

  const search = async () => {
    if (!query.trim() || busy) return
    setBusy(true)
    setError(null)
    setItems(null)
    try {
      setItems((await previewMemorySemantic(query.trim())).items)
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }

  return { query, setQuery, busy, error, setError, items, setItems, search }
}

export function UserMemorySemanticPanel() {
  const t = useTranslations("UserMemorySettings.semantic")
  const state = useSemanticSearch()
  return (
    <section className="space-y-2 border-y py-4" aria-label={t("title")}>
      <SemanticQueryForm state={state} />
      {state.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {state.error}
        </p>
      )}
      {state.items?.length === 0 && (
        <p className="text-xs text-muted-foreground">{t("empty")}</p>
      )}
      {state.items && (
        <ul className="divide-y">
          {state.items.map((item) => (
            <li key={item.id} className="break-words py-2 text-sm leading-6">
              {item.content}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function SemanticQueryForm({
  state,
}: {
  state: ReturnType<typeof useSemanticSearch>
}) {
  const t = useTranslations("UserMemorySettings.semantic")
  return (
    <form
      className="flex gap-2"
      aria-busy={state.busy}
      onSubmit={(event) => {
        event.preventDefault()
        void state.search()
      }}
    >
      <Input
        aria-label={t("query")}
        placeholder={t("query")}
        value={state.query}
        maxLength={MAX_QUERY_LENGTH}
        onChange={(event) => {
          state.setQuery(event.target.value)
          state.setItems(null)
          state.setError(null)
        }}
        disabled={state.busy}
      />
      <Button
        type="submit"
        size="icon"
        variant="outline"
        className="shrink-0"
        title={t("search")}
        aria-label={t("search")}
        disabled={state.busy || !state.query.trim()}
      >
        {state.busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Search className="size-4" />
        )}
      </Button>
    </form>
  )
}
