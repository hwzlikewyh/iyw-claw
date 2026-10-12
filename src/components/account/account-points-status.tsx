"use client"

import { AlertCircle, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { useIywAccount } from "@/contexts/iyw-account-context"
import type { IywAccountProfile } from "@/lib/types"

export function AccountPointsStatus({
  profile,
  compact = false,
}: {
  profile: IywAccountProfile | null
  compact?: boolean
}) {
  const t = useTranslations("AccountFlow")
  if (!profile?.points_stale) return null
  return (
    <div
      role="status"
      className={`flex gap-2 rounded-xl border px-3 py-2.5 text-[11px] leading-relaxed ${profile.points_requests_paused ? "border-amber-200/60 bg-amber-50/60 text-amber-800 dark:border-amber-800/50 dark:bg-amber-950/20 dark:text-amber-200" : "border-border/60 bg-muted/30 text-muted-foreground"}`}
    >
      <AlertCircle className="mt-0.5 size-3.5 shrink-0" />
      <div>
        <p className="font-medium">
          {t(profile.points_requests_paused ? "pointsPaused" : "pointsStale")}
        </p>
        {!compact ? (
          <p className="mt-0.5 opacity-80">
            {t(
              profile.points_requests_paused
                ? "pointsPausedHint"
                : "pointsCacheHint"
            )}
          </p>
        ) : null}
      </div>
    </div>
  )
}

export function AccountRequestsBanner() {
  const t = useTranslations("AccountFlow")
  const account = useIywAccount()
  const [refreshing, setRefreshing] = useState(false)
  if (
    !account.profile?.points_requests_paused &&
    !account.profile?.workspace_pending
  )
    return null
  const refresh = async () => {
    setRefreshing(true)
    try {
      await account.refreshProfile()
    } finally {
      setRefreshing(false)
    }
  }
  return (
    <div
      role="status"
      className="mx-auto flex w-full max-w-4xl items-center justify-between gap-3 rounded-xl border border-amber-200/60 bg-amber-50/50 px-4 py-3 text-xs text-amber-800 dark:border-amber-800/50 dark:bg-amber-950/20 dark:text-amber-200"
    >
      <span>
        {t(
          account.profile.workspace_pending
            ? "workspaceUnconfirmed"
            : "pointsPausedHint"
        )}
      </span>
      <Button
        size="sm"
        variant="ghost"
        disabled={refreshing}
        className="h-7 shrink-0 px-2 text-xs"
        onClick={() => void refresh()}
      >
        {refreshing ? (
          <Loader2 className="size-3 animate-spin" />
        ) : (
          <RefreshCw className="size-3" />
        )}
        {t("refresh")}
      </Button>
    </div>
  )
}
