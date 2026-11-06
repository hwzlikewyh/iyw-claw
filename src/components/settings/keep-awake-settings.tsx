"use client"

import { useCallback, useEffect, useState } from "react"
import { Coffee } from "lucide-react"
import { useTranslations } from "next-intl"
import { SettingRow, SettingSection } from "@/components/settings/settings-ui"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import { getKeepAwakeEnabled, setKeepAwakeEnabled } from "@/lib/keep-awake"

function useKeepAwakeSetting() {
  const [enabled, setEnabled] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      setEnabled(await getKeepAwakeEnabled())
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
      setEnabled(await setKeepAwakeEnabled(next))
    } catch (cause) {
      setError(toErrorMessage(cause))
    } finally {
      setSaving(false)
    }
  }, [])
  return { enabled, loading, saving, error, load, save }
}

export function KeepAwakeSettings() {
  const t = useTranslations("GeneralSettings")
  const setting = useKeepAwakeSetting()
  return (
    <SettingSection icon={Coffee} title={t("keepAwakeTitle")}>
      <SettingRow title={t("keepAwakeLabel")} description={t("keepAwakeHint")}>
        <Switch
          checked={setting.enabled}
          disabled={setting.loading || setting.saving || setting.error !== null}
          onCheckedChange={(checked) => void setting.save(checked)}
          aria-label={t("keepAwakeLabel")}
          aria-describedby={setting.error ? "keep-awake-error" : undefined}
        />
      </SettingRow>
      {setting.error && (
        <div
          id="keep-awake-error"
          role="alert"
          className="flex items-center gap-2 px-4 py-2 text-xs text-destructive"
        >
          <span>{t("keepAwakeFailed", { message: setting.error })}</span>
          <Button variant="ghost" size="sm" onClick={() => void setting.load()}>
            {t("keepAwakeRetry")}
          </Button>
        </div>
      )}
    </SettingSection>
  )
}
