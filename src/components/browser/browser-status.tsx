"use client"

import { CircleAlert, LoaderCircle, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { useBrowser } from "@/contexts/browser-context"
import { Button } from "@/components/ui/button"

export function BrowserStatus() {
  const t = useTranslations("Browser")
  const { state, error, busy, openBrowser } = useBrowser()
  const status =
    state?.runtime.status ?? state?.capability.status ?? "verifying"
  const loading =
    busy || ["verifying", "starting", "recovering"].includes(status)

  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-background px-6 text-center">
      <div className="max-w-sm">
        {loading ? (
          <LoaderCircle className="mx-auto mb-3 size-5 animate-spin text-muted-foreground" />
        ) : (
          <CircleAlert className="mx-auto mb-3 size-5 text-muted-foreground" />
        )}
        <div className="text-sm font-medium">{t(`runtime.${status}`)}</div>
        {error || state?.runtime.failureCode || state?.capability.reason ? (
          <div className="mt-1 text-xs text-muted-foreground">
            {t("runtimeUnavailable")}
          </div>
        ) : null}
        {!loading ? (
          <Button
            variant="outline"
            size="sm"
            className="mt-4"
            onClick={() => void openBrowser()}
          >
            <RefreshCw className="size-3.5" />
            {t("retry")}
          </Button>
        ) : null}
      </div>
    </div>
  )
}
