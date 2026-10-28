"use client"

import { AlertCircle, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { useState } from "react"
import { Button } from "@/components/ui/button"
import { useIywAccount } from "@/contexts/iyw-account-context"
import type { IywAccountProfile } from "@/lib/types"
import {
  hasAvailableAccountPoints,
  useAccountPointsInsufficient,
} from "./use-account-request-guard"

export function AccountPointsStatus({
  profile,
  compact = false,
}: {
  profile: IywAccountProfile | null
  compact?: boolean
}) {
  const t = useTranslations("AccountFlow")
  const points = useTranslations("Folder.conversation.pointsGate")
  if (!profile?.points_stale) return null
  const available = hasAvailableAccountPoints(profile.balance_points)
  return (
    <div
      role="status"
      className="flex gap-2 rounded-xl border border-border/60 bg-muted/30 px-3 py-2.5 text-[11px] leading-relaxed text-muted-foreground"
    >
      <AlertCircle className="mt-0.5 size-3.5 shrink-0" />
      <div>
        <p className="font-medium">
          {available ? t("pointsStale") : points("insufficientTitle")}
        </p>
        {!compact ? (
          <p className="mt-0.5 opacity-80">
            {available
              ? t("pointsCacheHint")
              : points("insufficientDescription")}
          </p>
        ) : null}
      </div>
    </div>
  )
}

export function AccountPointsNotice() {
  const t = useTranslations("Folder.conversation.pointsGate")
  const insufficient = useAccountPointsInsufficient()
  if (!insufficient) return null
  return (
    <span
      role="status"
      className="whitespace-nowrap text-xs text-destructive"
      title={t("insufficientDescription")}
    >
      {t("insufficientTitle")}
    </span>
  )
}

export function AccountRequestsBanner() {
  const t = useTranslations("AccountFlow")
  const account = useIywAccount()
  const [refreshing, setRefreshing] = useState(false)
  if (!account.profile?.workspace_pending) return null
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
      <span>{t("workspaceUnconfirmed")}</span>
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
