"use client"

import { useState } from "react"
import { ChevronDown } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

export interface AppProcessInfo {
  pid: number
  parentPid?: number
  displayName: string
  agentType: string | null
  isMainProcess: boolean
  cpuUsage: number
  memoryBytes: number
  privateMemoryBytes?: number
  groupId?: string
  groupDisplayName?: string
  processRole?: string
  status: string
}

export interface ProcessGroup {
  id: string
  displayName: string
  rootPid: number
  cpuUsage: number
  memoryBytes: number
  privateMemoryBytes?: number
  processes: AppProcessInfo[]
}

function fallbackGroup(
  proc: AppProcessInfo
): Pick<ProcessGroup, "id" | "displayName"> {
  if (proc.isMainProcess) return { id: "main", displayName: "iyw-claw" }
  if (proc.agentType) {
    return {
      id: `legacy-agent-${proc.pid}`,
      displayName: `${proc.displayName}会话`,
    }
  }
  return { id: "other", displayName: "其他附属进程" }
}

function groupRank(group: ProcessGroup): number {
  if (group.id === "main") return 0
  if (group.id.startsWith("webview2-")) return 1
  if (group.id.startsWith("agent-")) return 2
  return 3
}

export function buildProcessGroups(
  processes: AppProcessInfo[]
): ProcessGroup[] {
  const groups = new Map<string, ProcessGroup>()
  for (const proc of processes) {
    const fallback = fallbackGroup(proc)
    const id = proc.groupId ?? fallback.id
    const existing = groups.get(id) ?? {
      id,
      displayName: proc.groupDisplayName ?? fallback.displayName,
      rootPid: proc.pid,
      cpuUsage: 0,
      memoryBytes: 0,
      processes: [],
    }
    existing.cpuUsage += proc.cpuUsage
    existing.memoryBytes += proc.memoryBytes
    if (proc.privateMemoryBytes != null) {
      existing.privateMemoryBytes =
        (existing.privateMemoryBytes ?? 0) + proc.privateMemoryBytes
    }
    existing.processes.push(proc)
    if (proc.processRole === "main" || proc.processRole === "launcher") {
      existing.rootPid = proc.pid
    }
    groups.set(id, existing)
  }
  return Array.from(groups.values()).sort(
    (left, right) =>
      groupRank(left) - groupRank(right) ||
      right.memoryBytes - left.memoryBytes ||
      left.rootPid - right.rootPid
  )
}

function roleLabel(role: string | undefined): string {
  switch (role) {
    case "main":
      return "主进程"
    case "browser":
      return "浏览器"
    case "renderer":
      return "渲染器"
    case "gpu-process":
      return "GPU"
    case "utility":
      return "工具进程"
    case "crashpad-handler":
      return "崩溃处理"
    case "launcher":
      return "启动器"
    default:
      return "子进程"
  }
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

function ProcessRow({ proc }: { proc: AppProcessInfo }) {
  return (
    <div className="grid grid-cols-[minmax(220px,1fr)_110px_110px_72px_64px] items-center gap-3 px-4 py-2.5 hover:bg-muted/30">
      <div className="min-w-0">
        <div className="truncate text-sm font-medium">{proc.displayName}</div>
        <div className="truncate text-xs text-muted-foreground">
          PID {proc.pid}
          {proc.parentPid != null && ` · PPID ${proc.parentPid}`} ·{" "}
          {roleLabel(proc.processRole)}
        </div>
      </div>
      <div className="text-right text-sm tabular-nums">
        {formatBytes(proc.privateMemoryBytes)}
      </div>
      <div className="text-right text-sm tabular-nums text-muted-foreground">
        {formatBytes(proc.memoryBytes)}
      </div>
      <div className="text-right text-sm tabular-nums text-muted-foreground">
        {proc.cpuUsage.toFixed(1)}%
      </div>
      <Badge
        variant={proc.status === "运行中" ? "default" : "secondary"}
        className="w-14 justify-center text-xs"
      >
        {proc.status}
      </Badge>
    </div>
  )
}

function ProcessGroupHeader({
  group,
  collapsed,
  onToggle,
}: {
  group: ProcessGroup
  collapsed: boolean
  onToggle: () => void
}) {
  return (
    <button
      type="button"
      className="grid w-full grid-cols-[minmax(220px,1fr)_110px_110px_72px_64px] items-center gap-3 bg-muted/20 px-4 py-3 text-left hover:bg-muted/35"
      onClick={onToggle}
      aria-expanded={!collapsed}
    >
      <span className="flex min-w-0 items-center gap-2">
        <ChevronDown
          className={cn(
            "size-4 shrink-0 transition-transform",
            collapsed && "-rotate-90"
          )}
        />
        <span className="truncate text-sm font-semibold">
          {group.displayName}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">
          {group.processes.length} 个 · 根 PID {group.rootPid}
        </span>
      </span>
      <span className="text-right text-sm font-medium tabular-nums">
        {formatBytes(group.privateMemoryBytes)}
      </span>
      <span className="text-right text-sm tabular-nums text-muted-foreground">
        {formatBytes(group.memoryBytes)}
      </span>
      <span className="text-right text-sm tabular-nums text-muted-foreground">
        {group.cpuUsage.toFixed(1)}%
      </span>
      <span />
    </button>
  )
}

function ProcessGroupSection({
  group,
  collapsed,
  onToggle,
}: {
  group: ProcessGroup
  collapsed: boolean
  onToggle: () => void
}) {
  return (
    <section className="border-b last:border-b-0">
      <ProcessGroupHeader
        group={group}
        collapsed={collapsed}
        onToggle={onToggle}
      />
      {!collapsed && (
        <div className="divide-y">
          {group.processes.map((proc) => (
            <ProcessRow key={proc.pid} proc={proc} />
          ))}
        </div>
      )}
    </section>
  )
}

export function ProcessGroupList({ groups }: { groups: ProcessGroup[] }) {
  const [collapsedIds, setCollapsedIds] = useState<Set<string>>(new Set())
  const toggle = (groupId: string) => {
    setCollapsedIds((current) => {
      const next = new Set(current)
      if (next.has(groupId)) next.delete(groupId)
      else next.add(groupId)
      return next
    })
  }

  return (
    <div className="min-w-[720px]">
      <div className="grid grid-cols-[minmax(220px,1fr)_110px_110px_72px_64px] gap-3 border-b bg-muted/10 px-4 py-2 text-xs text-muted-foreground">
        <span>进程组 / 进程</span>
        <span className="text-right">私有提交</span>
        <span className="text-right">Working Set</span>
        <span className="text-right">CPU</span>
        <span>状态</span>
      </div>
      {groups.map((group) => (
        <ProcessGroupSection
          key={group.id}
          group={group}
          collapsed={collapsedIds.has(group.id)}
          onToggle={() => toggle(group.id)}
        />
      ))}
    </div>
  )
}
