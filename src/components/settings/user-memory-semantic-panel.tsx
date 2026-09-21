"use client"

import { useEffect, useState } from "react"
import { Database, Download, Loader2, Search } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import {
  getMemorySemanticStatus,
  prepareMemorySemantic,
  previewMemorySemantic,
  setMemorySemanticEnabled,
  type MemorySemanticStatus,
} from "@/lib/user-memory-entries"

const STATUS_INTERVAL_MS = 2000

function usePolledSemanticStatus() {
  const [status, setStatus] = useState<MemorySemanticStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let current = true
    const refresh = () =>
      getMemorySemanticStatus()
        .then((value) => {
          if (current) {
            setStatus(value)
            setError(null)
          }
        })
        .catch((reason) => current && setError(toErrorMessage(reason)))
    void refresh()
    const timer = setInterval(() => void refresh(), STATUS_INTERVAL_MS)
    return () => {
      current = false
      clearInterval(timer)
    }
  }, [])
  return { status, setStatus, error, setError }
}

function useSemanticStatus() {
  const { status, setStatus, error, setError } = usePolledSemanticStatus()
  const [saving, setSaving] = useState(false)
  const prepare = async () => {
    setError(null)
    try {
      await prepareMemorySemantic()
      setStatus(await getMemorySemanticStatus())
    } catch (reason) {
      setError(toErrorMessage(reason))
    }
  }
  const setEnabled = async (enabled: boolean) => {
    setSaving(true)
    setError(null)
    try {
      await setMemorySemanticEnabled(enabled)
      setStatus(await getMemorySemanticStatus())
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setSaving(false)
    }
  }
  return { status, error, prepare, saving, setEnabled }
}

export function UserMemorySemanticPanel() {
  const t = useTranslations("UserMemorySettings.semantic")
  const state = useSemanticStatus()
  const status = state.status
  const recovering = status?.modelDownloading || status?.retryPending
  const error = state.error || (!recovering && status?.lastError)
  return (
    <section className="space-y-3 border-y py-4">
      <SemanticHeader state={state} />
      <label className="flex items-center justify-between gap-4 text-sm">
        <span>{t("useInRecall")}</span>
        <Switch
          checked={status?.recallEnabled ?? false}
          disabled={!status?.supported || state.saving}
          onCheckedChange={(enabled) => void state.setEnabled(enabled)}
        />
      </label>
      {error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {error}
        </p>
      )}
      {status?.ready && (
        <p className="text-xs text-muted-foreground">
          {t("indexed")}: {status.indexedItems}
        </p>
      )}
      <SemanticSearch disabled={!status?.modelInstalled || status.busy} />
    </section>
  )
}

function SemanticHeader({
  state,
}: {
  state: ReturnType<typeof useSemanticStatus>
}) {
  const t = useTranslations("UserMemorySettings.semantic")
  const status = state.status
  const label = !status?.supported
    ? "unsupported"
    : status.busy
      ? "preparing"
      : status.modelDownloading
        ? "downloading"
        : status.retryPending
          ? "retryPending"
          : status.ready
            ? "ready"
            : status.modelInstalled
              ? "installed"
              : "missing"
  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 flex-wrap items-center gap-2">
        <Database className="size-4 shrink-0" />
        <h2 className="text-sm font-medium">{t("title")}</h2>
        {status && (
          <span className="text-xs text-muted-foreground">{t(label)}</span>
        )}
      </div>
      <Button
        size="sm"
        variant="outline"
        onClick={() => void state.prepare()}
        disabled={!status?.supported || status.busy || status.modelDownloading}
      >
        {status?.busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Download className="size-4" />
        )}
        {t(status?.modelInstalled ? "rebuild" : "prepare")}
      </Button>
    </div>
  )
}

function useSemanticSearch(disabled: boolean) {
  const [query, setQuery] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [items, setItems] = useState<
    Awaited<ReturnType<typeof previewMemorySemantic>>["items"] | null
  >(null)
  const search = async () => {
    if (!query.trim() || busy || disabled) return
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

function SemanticSearch({ disabled }: { disabled: boolean }) {
  const t = useTranslations("UserMemorySettings.semantic")
  const state = useSemanticSearch(disabled)
  return (
    <div className="space-y-2">
      <SemanticQueryForm disabled={disabled} state={state} />
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
    </div>
  )
}

function SemanticQueryForm({
  disabled,
  state,
}: {
  disabled: boolean
  state: ReturnType<typeof useSemanticSearch>
}) {
  const t = useTranslations("UserMemorySettings.semantic")
  const { query, setQuery, busy, setItems, search } = state
  return (
    <form
      className="flex gap-2"
      onSubmit={(event) => {
        event.preventDefault()
        void search()
      }}
    >
      <Input
        aria-label={t("query")}
        placeholder={t("query")}
        value={query}
        maxLength={512}
        onChange={(event) => {
          setQuery(event.target.value)
          setItems(null)
        }}
        disabled={disabled || busy}
      />
      <Button
        type="submit"
        size="icon"
        variant="outline"
        title={t("search")}
        aria-label={t("search")}
        disabled={disabled || busy || !query.trim()}
      >
        {busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Search className="size-4" />
        )}
      </Button>
    </form>
  )
}
