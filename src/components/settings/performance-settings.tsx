"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Activity, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { getShellTransport } from "@/lib/transport"
import {
  SettingsPageHeader,
  SettingsPageLayout,
} from "@/components/settings/settings-ui"
import {
  buildProcessGroups,
  ProcessGroupList,
  type AppProcessInfo,
  type ProcessGroup,
} from "@/components/settings/performance-process-groups"

const AUTO_REFRESH_INTERVAL_MS = 3000

interface AppPerformanceStats {
  cpuUsage: number
  memoryUsedBytes: number
  privateMemoryUsedBytes?: number
  osInfo: { osName: string; arch: string }
  processes: AppProcessInfo[]
}

interface PerformanceData {
  stats: AppPerformanceStats | null
  groups: ProcessGroup[]
  loading: boolean
  error: string | null
  autoRefresh: boolean
  lastUpdate: Date | null
  fetchStats: () => Promise<void>
  setAutoRefresh: (enabled: boolean) => void
}

function formatBytes(bytes: number | undefined): string {
  if (bytes == null) return "不可用"
  if (bytes === 0) return "0 B"
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  }
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

function useAutoRefresh(enabled: boolean, fetchStats: () => Promise<void>) {
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  useEffect(() => {
    if (enabled) {
      timerRef.current = setInterval(fetchStats, AUTO_REFRESH_INTERVAL_MS)
    } else if (timerRef.current) clearInterval(timerRef.current)
    return () => {
      if (timerRef.current) clearInterval(timerRef.current)
      timerRef.current = null
    }
  }, [enabled, fetchStats])
}

function usePerformanceData(): PerformanceData {
  const [stats, setStats] = useState<AppPerformanceStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [autoRefresh, setAutoRefresh] = useState(false)
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null)
  const fetchStats = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const result = await getShellTransport().call<AppPerformanceStats>(
        "get_performance_stats",
        {}
      )
      setStats(result)
      setLastUpdate(new Date())
    } catch (fetchError) {
      setError(
        fetchError instanceof Error ? fetchError.message : "获取性能数据失败"
      )
    } finally {
      setLoading(false)
    }
  }, [])
  useEffect(() => {
    void fetchStats()
  }, [fetchStats])
  const groups = useMemo(
    () => buildProcessGroups(stats?.processes ?? []),
    [stats?.processes]
  )
  useAutoRefresh(autoRefresh, fetchStats)
  return {
    stats,
    groups,
    loading,
    error,
    autoRefresh,
    lastUpdate,
    fetchStats,
    setAutoRefresh,
  }
}

function StatCard(props: {
  label: string
  value: string
  sub: string
  progress?: number
}) {
  return (
    <div className="space-y-2 rounded-lg border bg-card p-4">
      <div className="text-xs text-muted-foreground">{props.label}</div>
      <div className="text-2xl font-semibold tabular-nums">{props.value}</div>
      <div className="truncate text-xs text-muted-foreground">{props.sub}</div>
      {props.progress != null && (
        <div className="h-1 overflow-hidden rounded-full bg-muted">
          <div
            className="h-full rounded-full bg-primary transition-all duration-500"
            style={{
              width: `${Math.min(Math.max(props.progress, 0), 100)}%`,
            }}
          />
        </div>
      )}
    </div>
  )
}

function PerformanceHeader({ data }: { data: PerformanceData }) {
  return (
    <SettingsPageHeader
      icon={Activity}
      title="性能监控"
      action={
        <div className="flex items-center gap-3">
          <label className="flex cursor-pointer select-none items-center gap-2">
            <span className="text-sm text-muted-foreground">自动刷新</span>
            <Switch
              checked={data.autoRefresh}
              onCheckedChange={data.setAutoRefresh}
              aria-label="自动刷新"
            />
          </label>
          <Button
            variant="outline"
            size="sm"
            onClick={data.fetchStats}
            disabled={data.loading}
          >
            <RefreshCw
              className={cn("mr-1.5 size-3.5", data.loading && "animate-spin")}
            />
            刷新
          </Button>
        </div>
      }
    />
  )
}

function PerformanceStatsGrid({ data }: { data: PerformanceData }) {
  const { stats, groups } = data
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
      <StatCard
        label="应用私有提交"
        value={formatBytes(stats?.privateMemoryUsedBytes)}
        sub="各进程私有提交量合计"
      />
      <StatCard
        label="应用 Working Set"
        value={stats ? formatBytes(stats.memoryUsedBytes) : "不可用"}
        sub="驻留物理页，可能包含共享页"
      />
      <StatCard
        label="进程组"
        value={stats ? String(groups.length) : "不可用"}
        sub={stats ? `${stats.processes.length} 个进程` : "等待采集"}
      />
      <StatCard
        label="应用总 CPU"
        value={stats ? `${stats.cpuUsage.toFixed(1)}%` : "不可用"}
        sub="当前软件及附属进程"
        progress={stats?.cpuUsage}
      />
    </div>
  )
}

function PerformanceProcessPanel({ data }: { data: PerformanceData }) {
  const { stats, groups, loading, lastUpdate } = data
  return (
    <div className="overflow-hidden rounded-lg border">
      <div className="flex items-center justify-between border-b bg-muted/30 px-4 py-3">
        <span className="text-sm font-medium">
          应用进程
          {stats?.osInfo.osName && (
            <span className="ml-2 text-xs font-normal text-muted-foreground">
              {stats.osInfo.osName} · {stats.osInfo.arch}
            </span>
          )}
        </span>
        {lastUpdate && (
          <span className="text-xs text-muted-foreground">
            更新于 {lastUpdate.toLocaleTimeString("zh-CN")}
          </span>
        )}
      </div>
      {!stats || stats.processes.length === 0 ? (
        <div className="px-4 py-8 text-center text-sm text-muted-foreground">
          {loading ? "加载中..." : "暂无进程数据"}
        </div>
      ) : (
        <div className="overflow-x-auto">
          <ProcessGroupList groups={groups} />
        </div>
      )}
    </div>
  )
}

export function PerformanceSettings() {
  const data = usePerformanceData()
  return (
    <SettingsPageLayout>
      <PerformanceHeader data={data} />
      {data.error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {data.error}
        </div>
      )}
      <PerformanceStatsGrid data={data} />
      <PerformanceProcessPanel data={data} />
    </SettingsPageLayout>
  )
}
