"use client"

import { useState } from "react"
import { Loader2, Search } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toErrorMessage } from "@/lib/app-error"
import { previewMemorySemantic } from "@/lib/user-memory-entries"

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

  return { query, setQuery, busy, error, items, setItems, search }
}

export function UserMemorySemanticPanel() {
  const t = useTranslations("UserMemorySettings.semantic")
  const state = useSemanticSearch()
  return (
    <section className="space-y-2 border-y py-4">
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          void state.search()
        }}
      >
        <Input
          aria-label={t("query")}
          placeholder={t("query")}
          value={state.query}
          maxLength={512}
          onChange={(event) => {
            state.setQuery(event.target.value)
            state.setItems(null)
          }}
          disabled={state.busy}
        />
        <Button
          type="submit"
          size="icon"
          variant="outline"
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
