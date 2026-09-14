"use client"

import { CircleAlert, LoaderCircle, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { useBrowser } from "@/contexts/browser-context"
import { Button } from "@/components/ui/button"
import type { BrowserErrorEnvelope } from "@/lib/browser-types"

export function BrowserStatus({
  hostError,
  onRetry,
}: {
  hostError?: BrowserErrorEnvelope | null
  onRetry?: () => void
}) {
  const t = useTranslations("Browser")
  const { state, error: runtimeError, busy, openBrowser } = useBrowser()
  const error = hostError ?? runtimeError
  const failure =
    error?.message || state?.runtime.failureCode || state?.capability.reason
  const status = error
    ? "failed"
    : (state?.runtime.status ?? state?.capability.status ?? "verifying")
  const loading =
    !failure &&
    (busy || ["verifying", "starting", "recovering"].includes(status))
  const statusLabel =
    loading && busy && status !== "running"
      ? t("runtime.preparing")
      : t(`runtime.${status}`)

  return (
    <div className="flex h-full min-h-0 items-center justify-center bg-background px-6 text-center">
      <div className="min-w-0 max-w-sm" role="status">
        {loading ? (
          <LoaderCircle className="mx-auto mb-3 size-5 animate-spin text-muted-foreground" />
        ) : (
          <CircleAlert className="mx-auto mb-3 size-5 text-muted-foreground" />
        )}
        <div className="text-sm font-medium">{statusLabel}</div>
        {failure ? (
          <div className="mt-1 break-words text-xs text-muted-foreground">
            {t("runtimeUnavailable")}
            <div className="mt-1">{failure}</div>
          </div>
        ) : null}
        {!loading ? (
          <Button
            variant="outline"
            size="sm"
            className="mt-4"
            disabled={busy}
            onClick={() => {
              onRetry?.()
              void openBrowser()
            }}
          >
            <RefreshCw className="size-3.5" />
            {t("retry")}
          </Button>
        ) : null}
      </div>
    </div>
  )
}
