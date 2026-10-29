"use client"

import { useCallback, useEffect, useState } from "react"
import { RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { SettingRow, SettingSection } from "@/components/settings/settings-ui"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import {
  getAppUpdatePreferences,
  updateAppUpdatePreferences,
} from "@/lib/updater"

function useSilentAutoUpdateSetting() {
  const [enabled, setEnabled] = useState(true)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const preferences = await getAppUpdatePreferences()
      setEnabled(preferences.silentAutoUpdate)
    } catch (cause) {
      setError(toErrorMessage(cause))
    } finally {
      setLoading(false)
    }
  }, [])
  useEffect(() => {
    void load()
  }, [load])

  const save = useCallback(async (next: boolean) => {
    setSaving(true)
    setError(null)
    try {
      const preferences = await updateAppUpdatePreferences({
        silentAutoUpdate: next,
      })
      setEnabled(preferences.silentAutoUpdate)
    } catch (cause) {
      setError(toErrorMessage(cause))
    } finally {
      setSaving(false)
    }
  }, [])
  return { enabled, loading, saving, error, load, save }
}

export function SilentAutoUpdateSettings() {
  const t = useTranslations("GeneralSettings")
  const setting = useSilentAutoUpdateSetting()
  return (
    <SettingSection icon={RefreshCw} title={t("appUpdateTitle")}>
      <SettingRow
        title={t("silentAutoUpdateLabel")}
        description={t("silentAutoUpdateHint")}
      >
        <Switch
          checked={setting.enabled}
          disabled={setting.loading || setting.saving || setting.error !== null}
          onCheckedChange={(checked) => void setting.save(checked)}
          aria-label={t("silentAutoUpdateLabel")}
          aria-describedby={
            setting.error ? "silent-auto-update-error" : undefined
          }
        />
      </SettingRow>
      {setting.error && (
        <div
          id="silent-auto-update-error"
          role="alert"
          className="flex items-center gap-2 px-4 py-2 text-xs text-destructive"
        >
          <span>{t("silentAutoUpdateFailed", { message: setting.error })}</span>
          <Button variant="ghost" size="sm" onClick={() => void setting.load()}>
            {t("silentAutoUpdateRetry")}
          </Button>
        </div>
      )}
    </SettingSection>
  )
}
