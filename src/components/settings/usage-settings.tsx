"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { BarChart3, CalendarDays, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { getUsageDashboard } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import {
  isUsageSnapshotEmpty,
  ModelDistribution,
  UsageEmptyState,
  UsageSummary,
  UsageTodaySummary,
  type UsageSnapshot,
} from "@/components/settings/usage-settings-view"
import { UsageDailyChart } from "./usage-daily-chart"
import { UsageDailyTable } from "./usage-daily-table"
import { UsageConversations } from "./usage-conversations"
import {
  DEFAULT_USAGE_DAYS,
  USAGE_DAY_OPTIONS,
  usageCalendarDays,
} from "./usage-presentation"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"

function useUsageSnapshot(days: number) {
  const loadRunRef = useRef(0)
  const [result, setResult] = useState<{
    days: number
    snapshot: UsageSnapshot
  } | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    const loadRun = loadRunRef.current + 1
    loadRunRef.current = loadRun
    const isCurrent = () => loadRunRef.current === loadRun

    setLoading(true)
    setError(null)
    try {
      const stats = await getUsageDashboard(days)
      if (!isCurrent()) return
      setResult({ days, snapshot: { stats } })
    } catch (err) {
      if (isCurrent()) setError(toErrorMessage(err))
    } finally {
      if (isCurrent()) setLoading(false)
    }
  }, [days])

  useEffect(() => {
    load().catch((err) => {
      console.error("[UsageSettings] load failed:", err)
    })
    return () => {
      loadRunRef.current += 1
    }
  }, [load])

  return {
    snapshot: result?.days === days ? result.snapshot : null,
    loading,
    error,
    load,
  }
}

interface UsageControlsProps {
  days: number
  setDays: (days: number) => void
  loading: boolean
  refresh: () => void
}

function UsageControls({
  days,
  setDays,
  loading,
  refresh,
}: UsageControlsProps) {
  const t = useTranslations("UsageSettings")
  return (
    <div className="flex items-center gap-2">
      <Select
        value={String(days)}
        onValueChange={(value) => setDays(Number(value))}
      >
        <SelectTrigger
          size="sm"
          className="min-w-36 rounded-md"
          aria-label={t("period.label")}
        >
          <CalendarDays className="size-3.5" />
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {USAGE_DAY_OPTIONS.map((value) => (
            <SelectItem key={value} value={String(value)}>
              {value === 1
                ? t("period.today")
                : t("period.days", { days: value })}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            size="icon-sm"
            variant="outline"
            className="rounded-md"
            disabled={loading}
            onClick={refresh}
            aria-label={t("refresh")}
          >
            <RefreshCw
              className={loading ? "size-3.5 animate-spin" : "size-3.5"}
            />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t("refresh")}</TooltipContent>
      </Tooltip>
    </div>
  )
}

function UsageContent({
  snapshot,
  days,
}: {
  snapshot: UsageSnapshot
  days: number
}) {
  const t = useTranslations("UsageSettings")
  const rows = usageCalendarDays(snapshot.stats.dailyRows, days)
  return (
    <>
      {days !== 1 && <UsageTodaySummary row={rows[rows.length - 1]} />}
      <section
        className="space-y-3"
        aria-label={days === 1 ? t("period.today") : t("period.days", { days })}
      >
        {days !== 1 && (
          <h2 className="text-sm font-semibold">
            {t("period.days", { days })}
          </h2>
        )}
        <UsageSummary snapshot={snapshot} days={days} />
      </section>
      {isUsageSnapshotEmpty(snapshot) ? (
        <UsageEmptyState />
      ) : (
        <>
          <UsageDailyChart rows={rows} />
          <UsageDailyTable rows={rows} />
          <ModelDistribution rows={snapshot.stats.modelRows} />
        </>
      )}
    </>
  )
}

export function UsageSettings() {
  const t = useTranslations("UsageSettings")
  const [days, setDays] = useState<number>(DEFAULT_USAGE_DAYS)
  const [revision, setRevision] = useState(0)
  const { snapshot, loading, error, load } = useUsageSnapshot(days)

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={BarChart3}
        title={t("title")}
        action={
          <UsageControls
            days={days}
            setDays={setDays}
            loading={loading}
            refresh={() => {
              setRevision((value) => value + 1)
              void load()
            }}
          />
        }
      />

      {error && (
        <div
          role="alert"
          className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive"
        >
          {t("loadFailed", { message: error })}
        </div>
      )}

      {loading && !snapshot && (
        <div
          role="status"
          className="flex min-h-64 items-center justify-center gap-2 text-sm text-muted-foreground"
        >
          <Loader2 className="size-4 animate-spin" />
          {t("loading")}
        </div>
      )}
      {snapshot && (
        <div aria-busy={loading} className="space-y-5">
          <UsageContent snapshot={snapshot} days={days} />
        </div>
      )}
      <UsageConversations key={`${days}:${revision}`} days={days} />
    </SettingsPageLayout>
  )
}
