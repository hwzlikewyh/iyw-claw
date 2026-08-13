"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Activity, RefreshCw } from "lucide-react"
import { toast } from "sonner"
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
  type AppAgentSessionInfo,
  type AppProcessInfo,
  type ProcessGroup,
} from "@/components/settings/performance-process-groups"
import {
  MemoryGovernanceStatus,
  type AppSystemMemoryInfo,
} from "@/components/settings/performance-memory-status"

const AUTO_REFRESH_INTERVAL_MS = 3000

interface AppPerformanceStats {
  cpuUsage: number
  memoryUsedBytes: number
  privateMemoryUsedBytes?: number
  processes: AppProcessInfo[]
  agentSessions: AppAgentSessionInfo[]
  systemMemory?: AppSystemMemoryInfo
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
  endingConnectionIds: ReadonlySet<string>
  endSession: (connectionId: string) => Promise<void>
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

function usePerformanceStats() {
  const [stats, setStats] = useState<AppPerformanceStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
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
  return { stats, loading, error, lastUpdate, setError, fetchStats }
}

function removeConnectionId(current: Set<string>, connectionId: string) {
  const next = new Set(current)
  next.delete(connectionId)
  return next
}

function useEndAgentSession(
  fetchStats: () => Promise<void>,
  setError: (message: string | null) => void
) {
  const [endingConnectionIds, setEndingConnectionIds] = useState<Set<string>>(
    new Set()
  )
  const endSession = useCallback(
    async (connectionId: string) => {
      setEndingConnectionIds((current) => new Set(current).add(connectionId))
      try {
        const ended = await getShellTransport().call<boolean>(
          "end_agent_runtime_session",
          { connectionId }
        )
        await fetchStats()
        if (ended) toast.success("运行会话已结束，对话历史仍保留")
        else {
          setError("会话状态已变化，未结束")
          toast.info("会话状态已变化，未结束")
        }
      } catch (endError) {
        const message =
          endError instanceof Error ? endError.message : "结束运行会话失败"
        setError(message)
        toast.error(message)
      } finally {
        setEndingConnectionIds((current) =>
          removeConnectionId(current, connectionId)
        )
      }
    },
    [fetchStats, setError]
  )
  return { endingConnectionIds, endSession }
}

function usePerformanceData(): PerformanceData {
  const { stats, loading, error, lastUpdate, setError, fetchStats } =
    usePerformanceStats()
  const [autoRefresh, setAutoRefresh] = useState(false)
  const { endingConnectionIds, endSession } = useEndAgentSession(
    fetchStats,
    setError
  )
  useEffect(() => {
    void fetchStats()
  }, [fetchStats])
  const groups = useMemo(
    () =>
      buildProcessGroups(stats?.processes ?? [], stats?.agentSessions ?? []),
    [stats?.agentSessions, stats?.processes]
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
    endingConnectionIds,
    endSession,
  }
}

function SummaryMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 px-4 first:pl-0 last:pr-0">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 truncate text-xl font-semibold tabular-nums">
        {value}
      </div>
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

function PerformanceSummary({ data }: { data: PerformanceData }) {
  const { stats } = data
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-3 border-y py-3 sm:grid-cols-4">
      <SummaryMetric
        label="私有内存"
        value={stats ? formatBytes(stats.privateMemoryUsedBytes) : "不可用"}
      />
      <SummaryMetric
        label="系统可用内存"
        value={
          stats?.systemMemory
            ? formatBytes(stats.systemMemory.availableBytes)
            : "不可用"
        }
      />
      <SummaryMetric
        label="CPU 使用率"
        value={stats ? `${stats.cpuUsage.toFixed(1)}%` : "不可用"}
      />
      <SummaryMetric
        label="进程数"
        value={stats ? String(stats.processes.length) : "不可用"}
      />
    </div>
  )
}

function PerformanceProcessPanel({ data }: { data: PerformanceData }) {
  const { stats, groups, loading, lastUpdate } = data
  return (
    <div className="overflow-hidden rounded-lg border">
      <div className="flex items-center justify-between border-b bg-muted/30 px-4 py-3">
        <span className="text-sm font-medium">进程占用</span>
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
        <ProcessGroupList
          groups={groups}
          endingConnectionIds={data.endingConnectionIds}
          onEndSession={data.endSession}
        />
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
      <PerformanceSummary data={data} />
      <MemoryGovernanceStatus memory={data.stats?.systemMemory} />
      <PerformanceProcessPanel data={data} />
    </SettingsPageLayout>
  )
}
