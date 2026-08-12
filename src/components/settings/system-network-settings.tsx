"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Languages, Loader2, Settings, Wifi } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useAppI18n } from "@/components/i18n-provider"
import { BackupSettings } from "@/components/settings/backup-settings"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  getSystemProxySettings,
  updateSystemLanguageSettings,
  updateSystemProxySettings,
} from "@/lib/api"
import type { AppLocale } from "@/lib/types"
import { APP_LOCALES } from "@/lib/i18n"
import { toErrorMessage } from "@/lib/app-error"

const PROXY_EXAMPLE = "http://127.0.0.1:7890"
const APP_LANGUAGE_VALUES = APP_LOCALES
const SHOW_NETWORK_PROXY_SETTINGS = true

type LanguageSelectValue = "system" | AppLocale

function isAppLocale(value: string): value is AppLocale {
  return APP_LANGUAGE_VALUES.includes(value as AppLocale)
}

export function SystemNetworkSettings() {
  const t = useTranslations("SystemSettings")
  const tLanguage = useTranslations("Language")
  const { languageSettings, languageSettingsLoaded, setLanguageSettings } =
    useAppI18n()

  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [savingLanguage, setSavingLanguage] = useState(false)
  const [enabled, setEnabled] = useState(false)
  const [proxyUrl, setProxyUrl] = useState("")
  const [proxyUrlError, setProxyUrlError] = useState<string | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  const [appLanguage, setAppLanguage] = useState<LanguageSelectValue>(
    languageSettings.mode === "system" ? "system" : languageSettings.language
  )

  useEffect(() => {
    setAppLanguage(
      languageSettings.mode === "system" ? "system" : languageSettings.language
    )
  }, [languageSettings])

  const languageLabels = useMemo(
    () => ({
      en: tLanguage("english"),
      zh_cn: tLanguage("simplifiedChinese"),
    }),
    [tLanguage]
  )

  const loadSettings = useCallback(async () => {
    setLoading(true)
    setLoadError(null)

    try {
      const proxySettings = await getSystemProxySettings()
      setEnabled(proxySettings.enabled)
      setProxyUrl(proxySettings.proxy_url ?? "")
    } catch (err) {
      const message = toErrorMessage(err)
      setLoadError(message)
      console.error("[Settings] load system settings failed:", err)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadSettings().catch((err) => {
      console.error("[Settings] load system settings failed:", err)
    })
  }, [loadSettings])

  const saveProxySettings = useCallback(
    async (nextEnabled: boolean, nextProxyUrl: string) => {
      if (nextEnabled && !nextProxyUrl.trim()) return

      setSaving(true)
      try {
        const next = await updateSystemProxySettings({
          enabled: nextEnabled,
          proxy_url: nextProxyUrl.trim() || null,
        })
        setEnabled(next.enabled)
        setProxyUrl(next.proxy_url ?? "")
      } catch (err) {
        const message = toErrorMessage(err)
        toast.error(t("saveFailed", { message }))
      } finally {
        setSaving(false)
      }
    },
    [t]
  )

  const saveLanguage = useCallback(
    async (lang: LanguageSelectValue) => {
      setSavingLanguage(true)

      try {
        const next = await updateSystemLanguageSettings({
          mode: lang === "system" ? "system" : "manual",
          language: lang === "system" ? languageSettings.language : lang,
        })

        setLanguageSettings(next)
      } catch (err) {
        const message = toErrorMessage(err)
        toast.error(t("languageSaveFailed", { message }))
      } finally {
        setSavingLanguage(false)
      }
    },
    [languageSettings.language, setLanguageSettings, t]
  )

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground gap-2">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Settings}
        title={t("sectionTitle")}
        description={t("sectionDescription")}
      />

      {SHOW_NETWORK_PROXY_SETTINGS && (
        <section className="rounded-xl border bg-card p-4 space-y-4">
          <div className="flex items-center gap-2">
            <Wifi className="h-4 w-4 text-muted-foreground" />
            <h2 className="text-sm font-semibold">{t("proxyTitle")}</h2>
          </div>

          <p className="text-xs text-muted-foreground leading-5">
            {t("proxyDescription")}
          </p>

          {loadError && (
            <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
              {t("loadFailed", { message: loadError })}
            </div>
          )}

          <label className="inline-flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={enabled}
              disabled={saving}
              onChange={(event) => {
                const next = event.target.checked
                if (next && !proxyUrl.trim()) {
                  setProxyUrlError(t("proxyRequired"))
                  return
                }
                setProxyUrlError(null)
                setEnabled(next)
                saveProxySettings(next, proxyUrl)
              }}
            />
            {t("enableProxy")}
          </label>

          <div className="space-y-2">
            <label className="text-xs font-medium text-muted-foreground">
              {t("proxyAddress")}
            </label>
            <Input
              value={proxyUrl}
              onChange={(event) => {
                setProxyUrl(event.target.value)
                if (event.target.value.trim()) setProxyUrlError(null)
              }}
              onBlur={() => {
                if (enabled && !proxyUrl.trim()) {
                  setProxyUrlError(t("proxyRequired"))
                  return
                }
                setProxyUrlError(null)
                saveProxySettings(enabled, proxyUrl)
              }}
              placeholder={PROXY_EXAMPLE}
              disabled={saving}
              aria-invalid={proxyUrlError ? true : undefined}
            />
            {proxyUrlError && (
              <p className="text-[11px] text-destructive">{proxyUrlError}</p>
            )}
            <p className="text-[11px] text-muted-foreground">
              {t("proxyHint", { example: PROXY_EXAMPLE })}
            </p>
          </div>
        </section>
      )}

      <section className="rounded-xl border bg-card p-4 space-y-4">
        <div className="flex items-center gap-2">
          <Languages className="h-4 w-4 text-muted-foreground" />
          <h2 className="text-sm font-semibold">{t("languageTitle")}</h2>
        </div>

        <p className="text-xs text-muted-foreground leading-5">
          {t("languageDescription")}
        </p>

        <div className="space-y-2">
          <label className="text-xs font-medium text-muted-foreground">
            {t("appLanguage")}
          </label>
          <Select
            value={appLanguage}
            onValueChange={(value) => {
              let nextLang: LanguageSelectValue
              if (value === "system") {
                nextLang = "system"
              } else if (isAppLocale(value)) {
                nextLang = value
              } else {
                return
              }
              setAppLanguage(nextLang)
              saveLanguage(nextLang)
            }}
            disabled={savingLanguage || !languageSettingsLoaded}
          >
            <SelectTrigger className="w-full sm:w-56">
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="start">
              <SelectItem value="system">
                {tLanguage("followSystem")}
              </SelectItem>
              <SelectItem value="en">{languageLabels.en}</SelectItem>
              <SelectItem value="zh_cn">{languageLabels.zh_cn}</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </section>

      <BackupSettings />
    </SettingsPageLayout>
  )
}
