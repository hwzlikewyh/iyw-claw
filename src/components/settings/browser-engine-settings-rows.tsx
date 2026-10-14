"use client"

import { Download, LoaderCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { useBrowserEngineSettings } from "@/hooks/use-browser-engine-settings"
import { isDesktop } from "@/lib/transport"
import { SettingRow } from "./settings-ui"

export function BrowserEngineSettingsRows() {
  const t = useTranslations("GeneralSettings")
  const { settings, error, busy, installing, progress, save, install } =
    useBrowserEngineSettings()
  if (!isDesktop()) return null
  const loading = busy === "loading"
  const full =
    settings?.activeEngine !== "webview" && settings?.activeEngine !== undefined
  return (
    <>
      <SettingRow
        title={t("browserFullEngine")}
        description={t("browserFullEngineDescription")}
      >
        <Switch
          checked={settings?.fullBrowserEnabled ?? false}
          disabled={!settings || loading || busy === "saving"}
          onCheckedChange={(enabled) => void save(enabled)}
          aria-label={t("browserFullEngine")}
        />
      </SettingRow>
      <SettingRow
        title={t("browserActiveEngine")}
        description={
          settings?.pending
            ? t("browserEnginePending")
            : t("browserEngineUnified")
        }
      >
        <span className="text-xs text-muted-foreground" role="status">
          {loading
            ? t("browserEngineLoading")
            : full
              ? t("browserEngineFull")
              : t("browserEngineLight")}
        </span>
      </SettingRow>
      <SettingRow
        title={t("browserEngineInstallation")}
        description={
          settings?.fullBrowserInstalled
            ? t("browserEngineInstalled")
            : t("browserEngineNotInstalled")
        }
      >
        <Button
          size="sm"
          variant="outline"
          disabled={busy !== null || installing || !settings}
          onClick={() => void install()}
        >
          {installing ? (
            <LoaderCircle className="size-3.5 animate-spin" />
          ) : (
            <Download className="size-3.5" />
          )}
          {installing
            ? `${t("browserEngineInstalling")}${progress === null ? "" : ` ${progress}%`}`
            : settings?.fullBrowserInstalled
              ? t("browserEngineRepair")
              : t("browserEngineInstall")}
        </Button>
      </SettingRow>
      <SettingRow
        title={t("browserDataTitle")}
        description={t("browserDataDescription")}
      >
        <span className="text-xs text-muted-foreground">
          {t("browserDataPersistent")}
        </span>
      </SettingRow>
      {error || settings?.fullBrowserError ? (
        <div className="px-4 pb-3 text-xs text-destructive" role="alert">
          {error ?? settings?.fullBrowserError}
        </div>
      ) : null}
    </>
  )
}
