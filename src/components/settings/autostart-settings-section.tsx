"use client"

import { useCallback, useEffect, useState } from "react"
import { Power } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { SettingRow, SettingSection } from "@/components/settings/settings-ui"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import {
  disableAutostart,
  enableAutostart,
  isAutostartEnabled,
} from "@/lib/tauri"

function useAutostartSetting() {
  const t = useTranslations("GeneralSettings")
  const [enabled, setEnabled] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [loadError, setLoadError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    void isAutostartEnabled()
      .then((value) => {
        if (!cancelled) setEnabled(value)
      })
      .catch((error) => {
        if (!cancelled) setLoadError(toErrorMessage(error))
        console.error("[Settings] load autostart setting failed:", error)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  const save = useCallback(
    async (next: boolean) => {
      const previous = enabled
      setEnabled(next)
      setSaving(true)
      setLoadError(null)
      try {
        await (next ? enableAutostart() : disableAutostart())
      } catch (error) {
        setEnabled(previous)
        toast.error(
          t("autostartSaveFailed", { message: toErrorMessage(error) })
        )
      } finally {
        setSaving(false)
      }
    },
    [enabled, t]
  )

  return { enabled, loading, saving, loadError, save }
}

export function AutostartSettingsSection() {
  const t = useTranslations("GeneralSettings")
  const setting = useAutostartSetting()

  return (
    <SettingSection
      icon={Power}
      title={t("autostartTitle")}
      description={t("autostartDescription")}
    >
      <SettingRow title={t("autostartLabel")} description={t("autostartHint")}>
        <Switch
          checked={setting.enabled}
          disabled={setting.loading || setting.saving}
          onCheckedChange={(checked) => void setting.save(checked)}
          aria-label={t("autostartLabel")}
        />
      </SettingRow>
      {setting.loadError && (
        <div className="px-4 py-2 text-xs text-destructive">
          {t("loadFailed", { message: setting.loadError })}
        </div>
      )}
    </SettingSection>
  )
}
