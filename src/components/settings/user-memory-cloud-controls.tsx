"use client"

import { useEffect, useState } from "react"
import { RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import {
  getMemoryRetrievalModels,
  setMemoryCloudConfig,
  type CloudRetrievalConfig,
  type RetrievalModels,
} from "@/lib/user-memory-entries"

export function UserMemoryCloudControls({
  config,
  onUpdated,
}: {
  config: CloudRetrievalConfig
  onUpdated: () => Promise<void>
}) {
  const t = useTranslations("UserMemorySettings.semantic")
  const [models, setModels] = useState<RetrievalModels | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let current = true
    setModels(null)
    setError(null)
    getMemoryRetrievalModels()
      .then((value) => current && setModels(value))
      .catch((reason) => current && setError(toErrorMessage(reason)))
    return () => {
      current = false
    }
  }, [revision])
  const save = async (next: CloudRetrievalConfig) => {
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      await setMemoryCloudConfig(next)
      await onUpdated()
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="space-y-3">
      <div className="flex items-end gap-2">
        <ModelSelect
          automatic
          label={t("embeddingModel")}
          value={config.embeddingModel}
          models={models?.embeddings ?? []}
          disabled={busy || !models}
          onChange={(embeddingModel) =>
            void save({ ...config, embeddingModel })
          }
        />
        <Button
          size="icon"
          variant="outline"
          title={t("refreshModels")}
          aria-label={t("refreshModels")}
          disabled={busy}
          onClick={() => setRevision((value) => value + 1)}
        >
          <RefreshCw className="size-4" />
        </Button>
      </div>
      <label className="flex items-center justify-between gap-4 text-sm">
        <span>{t("rerankEnabled")}</span>
        <Switch
          checked={config.rerankEnabled}
          disabled={busy || (!models?.rerank.length && !config.rerankEnabled)}
          onCheckedChange={(rerankEnabled) =>
            void save({
              ...config,
              rerankEnabled,
              rerankModel: models?.rerank.some(
                (model) => model.id === config.rerankModel
              )
                ? config.rerankModel
                : models?.rerank[0]?.id || "",
            })
          }
        />
      </label>
      {config.rerankEnabled && (
        <ModelSelect
          label={t("rerankModel")}
          value={config.rerankModel}
          models={models?.rerank ?? []}
          disabled={busy || !models}
          onChange={(rerankModel) => void save({ ...config, rerankModel })}
        />
      )}
      <p className="text-xs text-muted-foreground">{t("cloudDataUse")}</p>
      {error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}

function ModelSelect({
  automatic = false,
  label,
  value,
  models,
  disabled,
  onChange,
}: {
  automatic?: boolean
  label: string
  value: string
  models: Array<{ id: string; displayName: string }>
  disabled: boolean
  onChange: (value: string) => void
}) {
  const t = useTranslations("UserMemorySettings.semantic")
  const unavailable = !!value && !models.some((model) => model.id === value)
  return (
    <label className="min-w-0 flex-1 space-y-1 text-sm">
      <span>{label}</span>
      <select
        className="h-9 w-full min-w-0 rounded-md border bg-background px-2 text-sm"
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="" disabled={!automatic}>
          {t(
            models.length
              ? automatic
                ? "automaticModel"
                : "chooseModel"
              : "noModels"
          )}
        </option>
        {unavailable && (
          <option value={value} disabled>
            {t("modelUnavailable")}
          </option>
        )}
        {models.map((model) => (
          <option key={model.id} value={model.id}>
            {model.displayName}
          </option>
        ))}
      </select>
    </label>
  )
}
