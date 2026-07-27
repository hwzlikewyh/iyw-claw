"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { BarChart3, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { getUsageDashboard } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import {
  DailyUsage,
  isUsageSnapshotEmpty,
  ModelDistribution,
  UsageEmptyState,
  UsageSummary,
  type UsageSnapshot,
} from "@/components/settings/usage-settings-view"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"

export function UsageSettings() {
  const t = useTranslations("UsageSettings")
  const loadRunRef = useRef(0)
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    const loadRun = loadRunRef.current + 1
    loadRunRef.current = loadRun
    const isCurrent = () => loadRunRef.current === loadRun

    setLoading(true)
    setError(null)
    try {
      const stats = await getUsageDashboard()
      if (!isCurrent()) return
      setSnapshot({ stats })
    } catch (err) {
      if (isCurrent()) setError(toErrorMessage(err))
    } finally {
      if (isCurrent()) setLoading(false)
    }
  }, [])

  useEffect(() => {
    load().catch((err) => {
      console.error("[UsageSettings] load failed:", err)
    })
    return () => {
      loadRunRef.current += 1
    }
  }, [load])

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={BarChart3}
        title={t("title")}
        description={t("description")}
        action={
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              setLoading(true)
              load().catch((err) => {
                console.error("[UsageSettings] refresh failed:", err)
              })
            }}
          >
            <RefreshCw className="h-3.5 w-3.5" />
            {t("refresh")}
          </Button>
        }
      />

      {error && (
        <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
          {t("loadFailed", { message: error })}
        </div>
      )}

      {snapshot && (
        <>
          <UsageSummary snapshot={snapshot} />
          {isUsageSnapshotEmpty(snapshot) ? (
            <UsageEmptyState />
          ) : (
            <>
              <ModelDistribution rows={snapshot.stats.modelRows} />
              <DailyUsage rows={snapshot.stats.dailyRows} />
            </>
          )}
        </>
      )}
    </SettingsPageLayout>
  )
}
