"use client"

import { useCallback, useEffect, useState } from "react"
import { Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  getArtifactNotificationMode,
  setArtifactNotificationMode,
  type ArtifactNotificationMode,
} from "@/lib/artifact-notifications"

const MODES: ArtifactNotificationMode[] = ["off", "auto", "always"]

function NotificationModeSelector({
  mode,
  failed,
  save,
  channelId,
}: Pick<ReturnType<typeof useNotificationMode>, "mode" | "failed" | "save"> & {
  channelId: number
}) {
  const t = useTranslations("ChatChannelSettings.artifactNotifications")
  return (
    <div className="flex w-full max-w-sm rounded-md bg-muted p-1">
      {MODES.map((value) => (
        <label key={value} className="flex min-w-0 flex-1 cursor-pointer">
          <input
            type="radio"
            name={`artifact-notification-mode-${channelId}`}
            className="peer sr-only"
            disabled={failed}
            value={value}
            checked={mode === value}
            onChange={() => void save(value)}
          />
          <span className="flex min-h-8 w-full items-center justify-center rounded px-2 text-center text-sm peer-checked:bg-background peer-checked:shadow-sm peer-focus-visible:ring-2 peer-focus-visible:ring-ring peer-disabled:cursor-default peer-disabled:opacity-50">
            {t(value)}
          </span>
        </label>
      ))}
    </div>
  )
}

function useNotificationMode(channelId: number) {
  const t = useTranslations("ChatChannelSettings.artifactNotifications")
  const [mode, setMode] = useState<ArtifactNotificationMode>("off")
  const [busy, setBusy] = useState(true)
  const [failed, setFailed] = useState(false)
  const load = useCallback(async () => {
    setBusy(true)
    try {
      setMode(await getArtifactNotificationMode(channelId))
      setFailed(false)
    } catch {
      setFailed(true)
    } finally {
      setBusy(false)
    }
  }, [channelId])
  useEffect(() => {
    void load()
  }, [load])
  const save = async (next: ArtifactNotificationMode) => {
    if (busy || next === mode) return
    setBusy(true)
    try {
      await setArtifactNotificationMode(channelId, next)
      setMode(next)
      toast.success(t("saved"))
    } catch {
      toast.error(t("saveFailed"))
    } finally {
      setBusy(false)
    }
  }
  return { mode, busy, failed, load, save }
}

export function ArtifactNotificationSettings({
  channelId,
}: {
  channelId: number
}) {
  const t = useTranslations("ChatChannelSettings.artifactNotifications")
  const { mode, busy, failed, load, save } = useNotificationMode(channelId)
  return (
    <fieldset
      className="mt-3 w-full min-w-0 space-y-2 sm:mt-0 sm:w-72 sm:shrink-0"
      disabled={busy}
      aria-busy={busy}
    >
      <legend className="mb-2 flex items-center gap-2 text-xs font-medium">
        {t("title")}
        {busy && (
          <Loader2 className="size-4 animate-spin" aria-label={t("loading")} />
        )}
      </legend>
      <NotificationModeSelector
        channelId={channelId}
        mode={mode}
        failed={failed}
        save={save}
      />
      {failed && (
        <div className="flex items-center gap-2 text-sm text-destructive">
          <span>{t("loadFailed")}</span>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            onClick={() => void load()}
            aria-label={t("retry")}
            title={t("retry")}
          >
            <RefreshCw className="size-4" />
          </Button>
        </div>
      )}
    </fieldset>
  )
}
