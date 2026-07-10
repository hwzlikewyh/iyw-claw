"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
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
    <ScrollArea className="h-full">
      <div className="w-full space-y-4 p-3 md:p-4">
        <section className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <h1 className="text-sm font-semibold">{t("title")}</h1>
            <p className="text-xs text-muted-foreground">{t("description")}</p>
          </div>
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
        </section>

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
      </div>
    </ScrollArea>
  )
}
