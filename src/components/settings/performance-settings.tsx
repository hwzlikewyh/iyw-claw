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
}

interface AppProcessInfo {
  pid: number
  displayName: string
  agentType: string | null
  isMainProcess: boolean
  cpuUsage: number
  memoryBytes: number
  status: string
}

interface AppPerformanceStats {
  cpuUsage: number
  memoryUsedBytes: number
  osInfo: OsInfo
  processes: AppProcessInfo[]
}

// ─── 工具函数 ─────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B"
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
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

function ProcessRow({ proc }: { proc: AppProcessInfo }) {
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
  const [stats, setStats] = useState<AppPerformanceStats | null>(null)
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
      const result = await transport.call<AppPerformanceStats>(
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
  const attachedProcs = stats?.processes.filter((p) => !p.isMainProcess) ?? []
  const attachedMemory = attachedProcs.reduce(
    (sum, proc) => sum + proc.memoryBytes,
    0
  )

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
          label="应用总 CPU"
          value={`${cpuPercent}%`}
          sub="当前软件及附属进程"
          progress={stats ? stats.cpuUsage : 0}
        />
        <StatCard
          label="应用总内存"
          value={stats ? formatBytes(stats.memoryUsedBytes) : "—"}
          sub="包含当前应用及附属进程"
        />
        <StatCard
          label="附属进程"
          value={String(attachedProcs.length)}
          sub={`占用 ${formatBytes(attachedMemory)}`}
        />
        <StatCard
          label="监控范围"
          value={stats ? `${stats.processes.length} 个进程` : "—"}
          sub="当前软件及附属进程"
        />
      </div>

      {/* 进程列表 */}
      <div className="rounded-lg border overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b bg-muted/30">
          <span className="text-sm font-medium">
            应用进程
            {stats?.osInfo.osName && (
              <span className="ml-2 text-xs text-muted-foreground font-normal">
                {stats.osInfo.osName} · {stats.osInfo.arch}
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

      {/* 附属进程内存汇总 */}
      {attachedProcs.length > 0 && (
        <div className="rounded-lg border overflow-hidden">
          <div className="px-4 py-3 border-b bg-muted/30">
            <span className="text-sm font-medium">附属进程内存汇总</span>
          </div>
          <div className="divide-y">
            {attachedProcs
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
                            attachedMemory > 0
                              ? `${((proc.memoryBytes / attachedMemory) * 100).toFixed(1)}%`
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
