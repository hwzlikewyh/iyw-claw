"use client"

import { useEffect, useState } from "react"
import { Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { getTransport } from "@/lib/transport"
import { toErrorMessage } from "@/lib/app-error"

interface LearningConfig {
  enabled: boolean
  model: string
  reviewEnabled: boolean
}
interface LearningStatus {
  config: LearningConfig
  available: boolean
  models: Array<{ id: string; displayName: string }>
}

function useLearningSettings(onUpdated: () => void) {
  const [status, setStatus] = useState<LearningStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let current = true
    getTransport()
      .call<LearningStatus>("get_user_memory_learning")
      .then((value) => current && setStatus(value))
      .catch((reason) => current && setError(toErrorMessage(reason)))
    return () => {
      current = false
    }
  }, [])
  const save = async (config: LearningConfig) => {
    if (!status || busy) return
    setBusy(true)
    setError(null)
    try {
      await getTransport().call("set_user_memory_learning", { config })
      setStatus({ ...status, config })
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  const refresh = async () => {
    setBusy(true)
    setError(null)
    try {
      await getTransport().call("refresh_user_memory_views")
      onUpdated()
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  return { status, busy, error, save, refresh }
}

export function UserMemoryLearningPanel({
  onUpdated,
}: {
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.learning")
  const state = useLearningSettings(onUpdated)
  const modelAvailable = (state.status?.models.length ?? 0) > 0
  return (
    <section className="space-y-3 border-y py-4">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <h2 className="text-sm font-medium">{t("title")}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t("dataUse")}</p>
        </div>
        <Switch
          aria-label={t("title")}
          checked={state.status?.config.enabled ?? false}
          disabled={
            !state.status ||
            state.busy ||
            ((!state.status.available || !modelAvailable) &&
              !state.status.config.enabled)
          }
          onCheckedChange={(enabled) =>
            state.status && void state.save({ ...state.status.config, enabled })
          }
        />
      </div>
      {state.status && <LearningControls state={state} />}
      {state.status && <ReviewToggle state={state} />}
      {state.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {state.error}
        </p>
      )}
    </section>
  )
}

function ReviewToggle({
  state,
}: {
  state: ReturnType<typeof useLearningSettings>
}) {
  const t = useTranslations("UserMemorySettings.maintenance")
  const status = state.status!
  return (
    <label className="flex items-center justify-between gap-4 text-sm">
      <span>{t("enableReview")}</span>
      <Switch
        checked={status.config.reviewEnabled ?? false}
        disabled={
          state.busy || (!status.config.enabled && !status.config.reviewEnabled)
        }
        onCheckedChange={(reviewEnabled) =>
          void state.save({ ...status.config, reviewEnabled })
        }
      />
    </label>
  )
}

function LearningControls({
  state,
}: {
  state: ReturnType<typeof useLearningSettings>
}) {
  const t = useTranslations("UserMemorySettings.learning")
  const status = state.status!
  return (
    <div className="flex justify-end">
      <Button
        variant="outline"
        size="sm"
        className="h-auto min-h-9 w-fit max-w-full whitespace-normal"
        disabled={
          state.busy ||
          !status.available ||
          !status.config.enabled ||
          status.models.length === 0
        }
        onClick={() => void state.refresh()}
      >
        {state.busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <RefreshCw className="size-4" />
        )}
        {t("refresh")}
      </Button>
    </div>
  )
}
