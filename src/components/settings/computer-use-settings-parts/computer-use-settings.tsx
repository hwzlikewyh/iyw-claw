"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Monitor } from "lucide-react"
import { toast } from "sonner"
import { ComputerSettingsSection } from "@/components/settings/computer-settings"
import { ComputerSharingSection } from "@/components/computer/computer-sharing-actions"
import { SettingsSection } from "@/components/computer/settings-layout"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Switch } from "@/components/ui/switch"
import { toErrorMessage } from "@/lib/app-error"
import {
  setComputerToolsEnabled,
  useComputerAvailable,
} from "@/lib/computer/computer-api"
import { useComputerEnabled } from "@/lib/computer/use-computer-enabled"
import { PermissionsSection } from "./permissions-section"

export function ComputerUseSettings() {
  const t = useTranslations("ComputerUse.settings")
  const desktop = useComputerAvailable()
  const { enabled, mark, applySince } = useComputerEnabled({
    desktopOnly: false,
  })
  const [switching, setSwitching] = useState(false)
  const setEnabled = async (next: boolean) => {
    setSwitching(true)
    const since = mark()
    try {
      applySince(await setComputerToolsEnabled(next), since)
    } catch (e) {
      toast.error(t("switch.failed"), { description: toErrorMessage(e) })
    } finally {
      setSwitching(false)
    }
  }
  return (
    <ScrollArea className="h-full">
      <div className="w-full space-y-4 p-3 md:p-4">
        <section className="space-y-1">
          <h1 className="text-sm font-semibold">{t("pageTitle")}</h1>
          <p className="text-xs text-muted-foreground">
            {t("pageDescription")}
          </p>
        </section>

        <SettingsSection
          icon={Monitor}
          title={t("switch.label")}
          description={t("switch.hint")}
          htmlFor="computer-use-enabled"
          control={
            <Switch
              id="computer-use-enabled"
              checked={enabled === true}
              onCheckedChange={(next) => void setEnabled(next)}
              disabled={
                enabled === null || switching || (!desktop && enabled !== true)
              }
            />
          }
        />

        {!desktop && (
          <p className="text-xs text-muted-foreground">{t("unavailable")}</p>
        )}

        {desktop && <PermissionsSection enabled={enabled === true} />}

        {desktop && <ComputerSharingSection />}

        <ComputerSettingsSection />
      </div>
    </ScrollArea>
  )
}
