"use client"

import { useCallback, useEffect, useState } from "react"
import { Gauge, Loader2, Save } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  type AgentConcurrencySettings,
  getAgentConcurrencySettings,
  setAgentConcurrencySettings,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

const MIN_LIMIT = 1
const MAX_LIMIT = 40

function normalizeLimit(value: string): number {
  const parsed = Number(value)
  if (!Number.isFinite(parsed)) return MAX_LIMIT
  return Math.min(MAX_LIMIT, Math.max(MIN_LIMIT, Math.round(parsed)))
}

function useConcurrencySettings() {
  const t = useTranslations("AcpAgentSettings.concurrency")
  const [settings, setSettings] = useState<AgentConcurrencySettings | null>(
    null
  )
  const [limit, setLimit] = useState(String(MAX_LIMIT))
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    void getAgentConcurrencySettings()
      .then((value) => {
        setSettings(value)
        setLimit(String(value.max_concurrent_subagents))
        setLoadError(null)
      })
      .catch((error) => setLoadError(toErrorMessage(error)))
      .finally(() => setLoading(false))
  }, [])

  const save = useCallback(async () => {
    if (!settings) return
    setSaving(true)
    try {
      const applied = await setAgentConcurrencySettings({
        ...settings,
        max_concurrent_subagents: normalizeLimit(limit),
      })
      setSettings(applied)
      setLimit(String(applied.max_concurrent_subagents))
      toast.success(t("saved"), { description: t("newSessions") })
    } catch (error) {
      toast.error(t("saveFailed"), { description: toErrorMessage(error) })
    } finally {
      setSaving(false)
    }
  }, [limit, settings, t])

  return {
    limit,
    loading,
    loadError,
    save,
    saving,
    setLimit,
    settings,
  }
}

interface LimitEditorProps {
  limit: string
  loading: boolean
  saving: boolean
  canSave: boolean
  onLimitChange: (value: string) => void
  onSave: () => void
}

function LimitEditor(props: LimitEditorProps) {
  const t = useTranslations("AcpAgentSettings.concurrency")
  return (
    <div className="flex flex-wrap items-end justify-between gap-3">
      <div className="min-w-0 space-y-1">
        <label
          htmlFor="agent-concurrency-limit"
          className="text-sm font-medium"
        >
          {t("limitLabel")}
        </label>
        <p className="text-xs text-muted-foreground">
          {t("limitHint", { min: MIN_LIMIT, max: MAX_LIMIT })}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <Input
          id="agent-concurrency-limit"
          type="number"
          min={MIN_LIMIT}
          max={MAX_LIMIT}
          step={1}
          value={props.limit}
          onChange={(event) => props.onLimitChange(event.target.value)}
          disabled={props.loading || props.saving}
          className="w-24"
        />
        <Button
          size="sm"
          onClick={props.onSave}
          disabled={props.loading || props.saving || !props.canSave}
        >
          {props.saving ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Save className="h-3.5 w-3.5" />
          )}
          {props.saving ? t("saving") : t("save")}
        </Button>
      </div>
    </div>
  )
}

export function AgentConcurrencySettingsSection() {
  const t = useTranslations("AcpAgentSettings.concurrency")
  const state = useConcurrencySettings()

  return (
    <section className="mb-3 space-y-3 rounded-lg border bg-card p-4">
      <div className="flex items-center gap-2">
        <Gauge className="h-4 w-4 text-muted-foreground" aria-hidden />
        <h2 className="text-sm font-semibold">{t("title")}</h2>
      </div>
      <p className="text-xs leading-5 text-muted-foreground">
        {t("description")}
      </p>

      {state.loadError ? (
        <p className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {t("loadFailed", { detail: state.loadError })}
        </p>
      ) : null}

      <LimitEditor
        limit={state.limit}
        loading={state.loading}
        saving={state.saving}
        canSave={Boolean(state.settings)}
        onLimitChange={state.setLimit}
        onSave={state.save}
      />
    </section>
  )
}
