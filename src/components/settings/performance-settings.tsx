"use client"

import { useState, useEffect, useCallback, useRef } from "react"
import { RefreshCw, Activity } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import { getShellTransport } from "@/lib/transport"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"

// ─── 类型定义（与 Rust serde camelCase 对应）───────────────────────────────

interface OsInfo {
  osName: string
  arch: string
  cpuCount: number
  uptimeSecs: number
}

interface AgentProcessInfo {
  pid: number
  displayName: string
  agentType: string | null
  cpuUsage: number
  memoryBytes: number
  status: string
}

interface SystemPerformanceStats {
  cpuUsage: number
  memoryUsedBytes: number
  memoryTotalBytes: number
  osInfo: OsInfo
  processes: AgentProcessInfo[]
}

// ─── 工具函数 ─────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B"
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400)
  const hours = Math.floor((secs % 86400) / 3600)
  const mins = Math.floor((secs % 3600) / 60)
  const parts: string[] = []
  if (days > 0) parts.push(`${days}d`)
  if (hours > 0) parts.push(`${hours}h`)
  if (mins > 0) parts.push(`${mins}m`)
  return parts.length > 0 ? parts.join(" ") : "< 1m"
}

// ─── StatCard ────────────────────────────────────────────────────────────

interface StatCardProps {
  label: string
  value: string
  sub?: string
  progress?: number
}

function StatCard({ label, value, sub, progress }: StatCardProps) {
  return (
    <div className="rounded-lg border bg-card p-4 space-y-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="text-2xl font-semibold tabular-nums">{value}</div>
      {sub && (
        <div className="text-xs text-muted-foreground truncate">{sub}</div>
      )}
      {progress !== undefined && (
        <div className="h-1 rounded-full bg-muted overflow-hidden">
          <div
            className="h-full rounded-full bg-primary transition-all duration-500"
            style={{
              width: `${Math.min(Math.max(progress, 0), 100).toFixed(1)}%`,
            }}
          />
        </div>
      )}
    </div>
  )
}

// ─── ProcessRow ──────────────────────────────────────────────────────────

function ProcessRow({ proc }: { proc: AgentProcessInfo }) {
  const isRunning = proc.status === "运行中"
  return (
    <div className="flex items-center gap-4 px-4 py-3 hover:bg-muted/40 transition-colors">
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium truncate">{proc.displayName}</div>
        <div className="text-xs text-muted-foreground">PID {proc.pid}</div>
      </div>
      <div className="w-16 text-right text-sm tabular-nums text-muted-foreground">
        {proc.cpuUsage.toFixed(1)}%
      </div>
      <div className="w-20 text-right text-sm tabular-nums">
        {formatBytes(proc.memoryBytes)}
      </div>
      <Badge
        variant={isRunning ? "default" : "secondary"}
        className="text-xs shrink-0 w-14 justify-center"
      >
        {proc.status}
      </Badge>
    </div>
  )
}

// ─── PerformanceSettings ─────────────────────────────────────────────────

export function PerformanceSettings() {
  const [stats, setStats] = useState<SystemPerformanceStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [autoRefresh, setAutoRefresh] = useState(false)
  const [lastUpdate, setLastUpdate] = useState<Date | null>(null)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const fetchStats = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const transport = getShellTransport()
      const result = await transport.call<SystemPerformanceStats>(
        "get_performance_stats",
        {}
      )
      setStats(result)
      setLastUpdate(new Date())
    } catch (e) {
      setError(e instanceof Error ? e.message : "获取性能数据失败")
    } finally {
      setLoading(false)
    }
  }, [])

  // 首次加载
  useEffect(() => {
    fetchStats()
  }, [fetchStats])

  // 自动刷新（3 秒间隔）
  useEffect(() => {
    if (autoRefresh) {
      timerRef.current = setInterval(fetchStats, 3000)
    } else if (timerRef.current) {
      clearInterval(timerRef.current)
      timerRef.current = null
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current)
    }
  }, [autoRefresh, fetchStats])

  const cpuPercent = stats ? stats.cpuUsage.toFixed(1) : "—"
  const memPercent = stats
    ? ((stats.memoryUsedBytes / stats.memoryTotalBytes) * 100).toFixed(1)
    : "—"
  const agentProcs = stats?.processes.filter((p) => p.agentType !== null) ?? []
  const totalAgentMemory = agentProcs.reduce((s, p) => s + p.memoryBytes, 0)

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Activity}
        title="性能监控"
        action={
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 cursor-pointer select-none">
              <span className="text-sm text-muted-foreground">自动刷新</span>
              <Switch
                checked={autoRefresh}
                onCheckedChange={setAutoRefresh}
                aria-label="自动刷新"
              />
            </label>
            <Button
              variant="outline"
              size="sm"
              onClick={fetchStats}
              disabled={loading}
            >
              <RefreshCw
                className={cn("h-3.5 w-3.5 mr-1.5", loading && "animate-spin")}
              />
              刷新
            </Button>
          </div>
        }
      />

      {/* 错误提示 */}
      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error}
        </div>
      )}

      {/* 统计卡片 */}
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard
          label="系统 CPU"
          value={`${cpuPercent}%`}
          progress={stats ? stats.cpuUsage : 0}
        />
        <StatCard
          label="系统内存"
          value={`${memPercent}%`}
          sub={
            stats
              ? `${formatBytes(stats.memoryUsedBytes)} / ${formatBytes(stats.memoryTotalBytes)}`
              : undefined
          }
          progress={
            stats ? (stats.memoryUsedBytes / stats.memoryTotalBytes) * 100 : 0
          }
        />
        <StatCard
          label="智能体进程"
          value={String(agentProcs.length)}
          sub={`占用 ${formatBytes(totalAgentMemory)}`}
        />
        <StatCard
          label="系统运行时间"
          value={stats ? formatUptime(stats.osInfo.uptimeSecs) : "—"}
          sub={
            stats
              ? `${stats.osInfo.cpuCount} 核 · ${stats.osInfo.arch}`
              : undefined
          }
        />
      </div>

      {/* 进程列表 */}
      <div className="rounded-lg border overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b bg-muted/30">
          <span className="text-sm font-medium">
            进程
            {stats?.osInfo.osName && (
              <span className="ml-2 text-xs text-muted-foreground font-normal">
                {stats.osInfo.osName} · {stats.osInfo.arch} · 运行{" "}
                {formatUptime(stats.osInfo.uptimeSecs)}
              </span>
            )}
          </span>
          {lastUpdate && (
            <span className="text-xs text-muted-foreground">
              更新于{" "}
              {lastUpdate.toLocaleTimeString("zh-CN", {
                hour: "2-digit",
                minute: "2-digit",
                second: "2-digit",
              })}
            </span>
          )}
        </div>

        {/* 表头 */}
        {stats && stats.processes.length > 0 && (
          <div className="flex items-center gap-4 px-4 py-2 border-b bg-muted/10">
            <div className="flex-1 text-xs text-muted-foreground">名称</div>
            <div className="w-16 text-right text-xs text-muted-foreground">
              CPU
            </div>
            <div className="w-20 text-right text-xs text-muted-foreground">
              内存
            </div>
            <div className="w-14 text-xs text-muted-foreground">状态</div>
          </div>
        )}

        {!stats || stats.processes.length === 0 ? (
          <div className="px-4 py-8 text-center text-sm text-muted-foreground">
            {loading ? "加载中..." : "暂无进程数据"}
          </div>
        ) : (
          <div className="divide-y">
            {stats.processes.map((proc) => (
              <ProcessRow key={proc.pid} proc={proc} />
            ))}
          </div>
        )}
      </div>

      {/* 智能体内存汇总 */}
      {agentProcs.length > 0 && (
        <div className="rounded-lg border overflow-hidden">
          <div className="px-4 py-3 border-b bg-muted/30">
            <span className="text-sm font-medium">智能体内存汇总</span>
          </div>
          <div className="divide-y">
            {agentProcs
              .slice()
              .sort((a, b) => b.memoryBytes - a.memoryBytes)
              .map((proc) => (
                <div
                  key={proc.pid}
                  className="flex items-center gap-4 px-4 py-2.5"
                >
                  <div className="flex-1 text-sm">{proc.displayName}</div>
                  <div className="text-sm tabular-nums text-muted-foreground">
                    {formatBytes(proc.memoryBytes)}
                  </div>
                  <div className="w-32">
                    <div className="h-1.5 rounded-full bg-muted overflow-hidden">
                      <div
                        className="h-full rounded-full bg-blue-500/70 transition-all duration-500"
                        style={{
                          width:
                            totalAgentMemory > 0
                              ? `${((proc.memoryBytes / totalAgentMemory) * 100).toFixed(1)}%`
                              : "0%",
                        }}
                      />
                    </div>
                  </div>
                </div>
              ))}
          </div>
        </div>
      )}
    </SettingsPageLayout>
  )
}
