"use client"

import { useState } from "react"
import { ChevronDown } from "lucide-react"
import { cn } from "@/lib/utils"

export interface AppProcessInfo {
  pid: number
  displayName: string
  agentType: string | null
  isMainProcess: boolean
  cpuUsage: number
  memoryBytes: number
  groupId?: string
  groupDisplayName?: string
  processRole?: string
}

export interface ProcessGroup {
  id: string
  displayName: string
  rootPid: number
  cpuUsage: number
  memoryBytes: number
  isAgentSession: boolean
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
      isAgentSession: Boolean(proc.agentType),
      processes: [],
    }
    existing.cpuUsage += proc.cpuUsage
    existing.memoryBytes += proc.memoryBytes
    if (proc.agentType) existing.isAgentSession = true
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
    <div className="grid grid-cols-[minmax(0,1fr)_6rem_4rem] items-center gap-2 px-4 py-2.5 hover:bg-muted/30">
      <div className="min-w-0 pl-6">
        <div className="truncate text-sm font-medium">{proc.displayName}</div>
        <div className="truncate text-xs text-muted-foreground">
          {roleLabel(proc.processRole)} · PID {proc.pid}
        </div>
      </div>
      <div className="text-right text-sm tabular-nums text-muted-foreground">
        {formatBytes(proc.memoryBytes)}
      </div>
      <div className="text-right text-sm tabular-nums text-muted-foreground">
        {proc.cpuUsage.toFixed(1)}%
      </div>
    </div>
  )
}

function ProcessGroupHeaderContent({
  group,
  expanded,
  expandable,
}: {
  group: ProcessGroup
  expanded: boolean
  expandable: boolean
}) {
  return (
    <>
      <span className="flex min-w-0 items-center gap-2">
        {expandable ? (
          <ChevronDown
            className={cn(
              "size-4 shrink-0 transition-transform",
              !expanded && "-rotate-90"
            )}
          />
        ) : (
          <span aria-hidden className="size-4 shrink-0" />
        )}
        <span className="truncate text-sm font-semibold">
          {group.displayName}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">
          {group.processes.length} 个进程
        </span>
      </span>
      <span className="text-right text-sm font-medium tabular-nums">
        {formatBytes(group.memoryBytes)}
      </span>
      <span className="text-right text-sm tabular-nums text-muted-foreground">
        {group.cpuUsage.toFixed(1)}%
      </span>
    </>
  )
}

function ProcessGroupHeader({
  group,
  expanded,
  expandable,
  onToggle,
}: {
  group: ProcessGroup
  expanded: boolean
  expandable: boolean
  onToggle: () => void
}) {
  const className =
    "grid w-full grid-cols-[minmax(0,1fr)_6rem_4rem] items-center gap-2 bg-muted/20 px-4 py-3 text-left"

  if (!expandable) {
    return (
      <div className={className}>
        <ProcessGroupHeaderContent {...{ group, expanded, expandable }} />
      </div>
    )
  }

  return (
    <button
      type="button"
      className={cn(className, "hover:bg-muted/35")}
      onClick={onToggle}
      aria-expanded={expanded}
    >
      <ProcessGroupHeaderContent {...{ group, expanded, expandable }} />
    </button>
  )
}

function ProcessGroupSection({
  group,
  expanded,
  onToggle,
}: {
  group: ProcessGroup
  expanded: boolean
  onToggle: () => void
}) {
  const expandable = !group.isAgentSession

  return (
    <section className="border-b last:border-b-0">
      <ProcessGroupHeader
        group={group}
        expanded={expanded}
        expandable={expandable}
        onToggle={onToggle}
      />
      {expandable && expanded && (
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
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set())
  const toggle = (groupId: string) => {
    setExpandedIds((current) => {
      const next = new Set(current)
      if (next.has(groupId)) next.delete(groupId)
      else next.add(groupId)
      return next
    })
  }

  return (
    <div>
      <div className="grid grid-cols-[minmax(0,1fr)_6rem_4rem] gap-2 border-b bg-muted/10 px-4 py-2 text-xs text-muted-foreground">
        <span>进程组</span>
        <span className="text-right">内存</span>
        <span className="text-right">CPU</span>
      </div>
      {groups.map((group) => (
        <ProcessGroupSection
          key={group.id}
          group={group}
          expanded={expandedIds.has(group.id)}
          onToggle={() => toggle(group.id)}
        />
      ))}
    </div>
  )
}
