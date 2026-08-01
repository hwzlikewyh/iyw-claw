"use client"

import { useEffect } from "react"

import { Progress } from "@/components/ui/progress"
import { useBootstrapInit } from "@/hooks/use-bootstrap-init"

// 阶段与组件文案内联在此组件中，避免新增 i18n key 与并行任务冲突；
// 后续 Task 13 接线时可迁移到 messages 目录。
const PHASE_LABELS: Record<string, string> = {
  not_started: "未开始",
  resolving: "解析版本中",
  downloading: "下载中",
  verifying: "校验中",
  staging: "暂存中",
  activating: "激活中",
  health_check: "健康检查",
  ready: "就绪",
  degraded: "离线降级",
  retryable: "重试中",
  blocked: "已阻塞",
}

const COMPONENT_LABELS: Record<string, string> = {
  node: "Node.js",
  git: "Git",
  uv: "uv",
}

const WRITER_BUSY_TEXT = "另一个窗口正在进行初始化，本窗口只订阅进度。"
const OFFLINE_TEXT = "当前离线，使用最近一次成功库存运行。"

/**
 * 托管初始化状态展示（非 Skill 市场 UI）。
 * 订阅 `app://bootstrap-init` 进度事件并展示阶段、组件、字节、速率与 ETA；
 * 离线 / degraded / blocked 状态给出可读提示。初始化动作由门禁流程触发，
 * 本组件只读展示。
 */
export function BootstrapInitStatus({
  taskId,
  className,
}: {
  taskId: string
  className?: string
}) {
  const { state, percent, start, refreshStatus } = useBootstrapInit(taskId)

  useEffect(() => {
    void start()
    void refreshStatus()
    return () => undefined
  }, [start, refreshStatus])

  if (state.phase === "not_started" || state.phase === "ready") {
    return null
  }

  const formatBytes = (value: number | null): string => {
    if (value == null) return "—"
    const units = ["B", "KB", "MB", "GB"]
    let index = 0
    let current = value
    while (current >= 1024 && index < units.length - 1) {
      current /= 1024
      index += 1
    }
    return `${current.toFixed(current >= 100 || index === 0 ? 0 : 1)} ${units[index]}`
  }

  const rate =
    state.rateBps != null ? `${formatBytes(state.rateBps)}/s` : null
  const eta =
    state.etaSecs != null && state.etaSecs > 0
      ? `≈${Math.ceil(state.etaSecs / 60)}m`
      : null
  const component = state.component
    ? (COMPONENT_LABELS[state.component] ?? state.component)
    : null

  return (
    <div className={`grid gap-2 text-sm ${className ?? ""}`}>
      <div className="flex items-center justify-between gap-3">
        <span className="text-muted-foreground">
          {PHASE_LABELS[state.phase] ?? state.phase}
          {component ? ` · ${component}` : null}
        </span>
        <span className="tabular-nums text-muted-foreground">
          {state.downloaded != null && state.total != null
            ? `${formatBytes(state.downloaded)} / ${formatBytes(state.total)}`
            : null}
          {rate ? ` · ${rate}` : null}
          {eta ? ` · ${eta}` : null}
        </span>
      </div>
      {percent != null ? <Progress value={percent} className="h-1.5" /> : null}
      {state.offline || state.writerBusy || state.lastError ? (
        <p className="text-xs text-amber-600">
          {state.writerBusy
            ? WRITER_BUSY_TEXT
            : state.offline
              ? OFFLINE_TEXT
              : (state.lastError ?? "")}
        </p>
      ) : null}
    </div>
  )
}
