"use client"

import { useState } from "react"
import { LayoutDashboard } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Switch } from "@/components/ui/switch"
import {
  useUiSettings,
  updateUiSettings,
  reloadUiSettings,
} from "@/lib/intelligent-ui/settings"
import { Button } from "@/components/ui/button"
import { SettingRow, SettingSection, SettingSectionBody } from "./settings-ui"

export function IntelligentUiSettings() {
  const t = useTranslations("IntelligentUI")
  const settings = useUiSettings()
  const [saving, setSaving] = useState(false)
  async function change(enabled: boolean) {
    setSaving(true)
    try {
      await updateUiSettings(enabled)
    } catch (error) {
      toast.error(t("saveFailed"), { description: String(error) })
    } finally {
      setSaving(false)
    }
  }
  return (
    <SettingSection
      icon={LayoutDashboard}
      title={t("title")}
      description={t("description")}
    >
      <SettingSectionBody>
        <SettingRow title={t("enabled")} description={t("enabledDescription")}>
          <Switch
            aria-label={t("enabled")}
            checked={settings.enabled}
            disabled={!settings.loaded || saving || Boolean(settings.error)}
            onCheckedChange={(value) => void change(value)}
          />
        </SettingRow>
        {settings.error && (
          <p role="alert" className="text-sm text-destructive">
            {t("loadFailed")}{" "}
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void reloadUiSettings()}
            >
              {t("retry")}
            </Button>
          </p>
        )}
      </SettingSectionBody>
    </SettingSection>
  )
}
