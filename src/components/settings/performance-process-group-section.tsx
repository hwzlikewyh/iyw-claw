"use client"

import { ChevronDown, Power } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type {
  AppAgentSessionInfo,
  AppProcessInfo,
  ProcessGroup,
} from "@/components/settings/performance-process-groups"

function formatBytes(bytes: number | undefined): string {
  if (bytes == null) return "不可用"
  if (bytes === 0) return "0 B"
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  }
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}

function roleLabel(role: string | undefined): string {
  const labels: Record<string, string> = {
    main: "主进程",
    browser: "浏览器",
    renderer: "渲染器",
    "gpu-process": "GPU",
    utility: "工具进程",
    "crashpad-handler": "崩溃处理",
    launcher: "启动器",
  }
  return role ? (labels[role] ?? "子进程") : "子进程"
}

function ProcessRow({ proc }: { proc: AppProcessInfo }) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_6rem_6rem_4rem] items-center gap-2 px-4 py-2.5 hover:bg-muted/30">
      <div className="min-w-0 pl-6">
        <div className="truncate text-sm font-medium">{proc.displayName}</div>
        <div className="truncate text-xs text-muted-foreground">
          {roleLabel(proc.processRole)} · PID {proc.pid}
        </div>
      </div>
      <div className="text-right text-sm tabular-nums text-muted-foreground">
        {formatBytes(proc.privateMemoryBytes)}
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

function ProcessGroupHeader({
  group,
  expanded,
  onToggle,
}: {
  group: ProcessGroup
  expanded: boolean
  onToggle: () => void
}) {
  return (
    <button
      type="button"
      className="grid w-full grid-cols-[minmax(0,1fr)_6rem_6rem_4rem] items-center gap-2 bg-muted/20 px-4 py-3 text-left hover:bg-muted/35"
      onClick={onToggle}
      aria-expanded={expanded}
    >
      <span className="flex min-w-0 items-center gap-2">
        <ChevronDown
          className={cn(
            "size-4 shrink-0 transition-transform",
            !expanded && "-rotate-90"
          )}
        />
        <span className="truncate text-sm font-semibold">
          {group.displayName}
        </span>
        <span className="shrink-0 text-xs text-muted-foreground">
          {group.processes.length} 个进程
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
    </button>
  )
}

function sessionStatusLabel(session: AppAgentSessionInfo): string {
  if (session.status === "connecting") return "正在恢复"
  if (session.status === "prompting") return "运行中"
  if (!session.recoverable) return "不支持恢复"
  if (session.protectionReason) return "受保护"
  return "可自动恢复"
}

function protectionReasonLabel(reason: string): string {
  const labels: Record<string, string> = {
    connection_not_idle: "连接未空闲",
    session_not_linked: "会话尚未绑定",
    session_not_recoverable: "Agent 不支持可靠恢复",
    session_recovery_failed: "最近恢复失败",
    turn_in_progress: "正在生成或结算",
    permission_pending: "等待权限确认",
    question_pending: "等待用户回答",
    confirmation_pending: "等待渠道确认",
    background_turn_active: "后台回合运行中",
    tool_call_active: "工具调用运行中",
    delegation_active: "子 Agent 运行中",
    background_task_active: "后台任务运行中",
    terminal_active: "终端任务运行中",
    agent_input_pending: "消息队列待处理",
    client_input_pending: "存在未发送内容",
    conversation_visible: "对话当前可见",
    recently_active: "最近使用保护期",
  }
  return labels[reason] ?? reason
}

function AgentSessionDetail({
  session,
  ending,
  onEndSession,
}: {
  session: AppAgentSessionInfo
  ending: boolean
  onEndSession?: (connectionId: string) => void
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
      <div className="min-w-0">
        <div className="truncate text-sm font-medium">
          {session.conversationTitle || "未命名对话"}
        </div>
        <div className="mt-0.5 text-xs text-muted-foreground">
          {sessionStatusLabel(session)}
          {session.protectionReason
            ? ` · ${protectionReasonLabel(session.protectionReason)}`
            : " · 可安全结束"}
        </div>
      </div>
      {session.canEnd && onEndSession && (
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={ending}
          onClick={() => onEndSession(session.connectionId)}
        >
          <Power className="size-3.5" />
          {ending ? "正在结束" : "结束运行会话"}
        </Button>
      )}
    </div>
  )
}

export function ProcessGroupSection({
  group,
  expanded,
  onToggle,
  ending,
  onEndSession,
}: {
  group: ProcessGroup
  expanded: boolean
  onToggle: () => void
  ending: boolean
  onEndSession?: (connectionId: string) => void
}) {
  const session = group.session
  return (
    <section className="border-b last:border-b-0">
      <ProcessGroupHeader {...{ group, expanded, onToggle }} />
      {expanded && (
        <div className="divide-y">
          {session && (
            <AgentSessionDetail {...{ session, ending, onEndSession }} />
          )}
          {group.processes.map((proc) => (
            <ProcessRow key={proc.pid} proc={proc} />
          ))}
        </div>
      )}
    </section>
  )
}
