"use client"

import { Globe2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Switch } from "@/components/ui/switch"
import {
  useBrowserVisibility,
  writeBrowserVisibility,
} from "@/hooks/use-browser-visibility"
import { toErrorMessage } from "@/lib/app-error"
import { SettingRow, SettingSection } from "./settings-ui"

export function BrowserSettingsSection() {
  const t = useTranslations("GeneralSettings")
  const visible = useBrowserVisibility()

  const saveVisibility = (next: boolean) => {
    try {
      writeBrowserVisibility(next)
    } catch (error) {
      console.error("[Browser] failed to save visibility preference", error)
      toast.error(t("browserSaveFailed", { message: toErrorMessage(error) }))
    }
  }

  return (
    <SettingSection icon={Globe2} title={t("browserTitle")}>
      <SettingRow
        title={t("browserVisible")}
        description={t("browserDescription")}
      >
        <Switch
          checked={visible}
          onCheckedChange={saveVisibility}
          aria-label={t("browserVisible")}
        />
      </SettingRow>
    </SettingSection>
  )
}
