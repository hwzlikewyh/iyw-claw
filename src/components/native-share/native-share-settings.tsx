"use client"

import { useCallback, useEffect, useState, useSyncExternalStore } from "react"
import { Loader2, Share2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { SettingRow, SettingSection } from "@/components/settings/settings-ui"
import { isLocalDesktop } from "@/lib/platform"
import {
  nativeShareStatus,
  registerNativeShare,
  shareError,
  type ShareStatus,
} from "@/lib/native-share"
import { NativeShareButton } from "./native-share-button"

const subscribeDesktop = () => () => {}

export function NativeShareSettings() {
  const t = useTranslations("NativeShare")
  const [status, setStatus] = useState<ShareStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const local = useSyncExternalStore(
    subscribeDesktop,
    isLocalDesktop,
    () => false
  )
  const refresh = useCallback(async () => {
    setError(null)
    try {
      setStatus(await nativeShareStatus())
    } catch (cause) {
      setError(shareError(cause))
    }
  }, [])
  useEffect(() => {
    if (local) void refresh()
  }, [local, refresh])
  if (!local) return null
  const register = async () => {
    setBusy(true)
    setError(null)
    try {
      await registerNativeShare()
      await refresh()
    } catch (cause) {
      setError(shareError(cause))
    } finally {
      setBusy(false)
    }
  }
  return (
    <SettingSection icon={Share2} title={t("title")}>
      <SettingRow
        title={t("systemSharing")}
        description={t(status?.installed ? "installed" : "notInstalled")}
      >
        <div className="flex flex-wrap items-center gap-2">
          {status?.canRegister && !status.installed && (
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              onClick={() => void register()}
            >
              {busy && <Loader2 className="size-3.5 animate-spin" />}
              {t("enableSharing")}
            </Button>
          )}
          <NativeShareButton />
        </div>
      </SettingRow>
      <p className="px-4 pb-3 text-xs leading-relaxed text-muted-foreground">
        {t("platformSupport")}
      </p>
      {error && (
        <p role="alert" className="px-4 pb-3 text-sm text-destructive">
          {error}
        </p>
      )}
    </SettingSection>
  )
}
