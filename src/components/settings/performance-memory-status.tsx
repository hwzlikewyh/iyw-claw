"use client"

import { MemoryStick } from "lucide-react"
import { cn } from "@/lib/utils"

export interface AppSystemMemoryInfo {
  totalBytes: number
  availableBytes: number
  pressure: "comfortable" | "shrinking" | "emergency" | "unknown"
  shrinkingReserveBytes: number
  emergencyReserveBytes: number
  idleAgentBudgetBytes: number
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / 1024 / 1024).toFixed(0)} MB`
  }
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`
}

function pressureCopy(pressure: AppSystemMemoryInfo["pressure"]) {
  switch (pressure) {
    case "comfortable":
      return {
        label: "工具空间充足",
        detail: "空闲 Agent 按常规数量和内存预算保留。",
        color: "text-emerald-600 dark:text-emerald-400",
      }
    case "shrinking":
      return {
        label: "正在预留工具空间",
        detail: "仅保留最近使用的空闲 Agent，并逐个释放更早的运行态。",
        color: "text-amber-600 dark:text-amber-400",
      }
    case "emergency":
      return {
        label: "系统内存紧张",
        detail:
          "逐个释放可恢复、不可见且空闲的 Agent；对话历史和草稿仍会保留。",
        color: "text-destructive",
      }
    default:
      return {
        label: "系统内存状态不可用",
        detail: "暂时按常规空闲策略运行，不基于系统压力主动回收。",
        color: "text-muted-foreground",
      }
  }
}

export function MemoryGovernanceStatus({
  memory,
}: {
  memory?: AppSystemMemoryInfo
}) {
  if (!memory) return null
  const copy = pressureCopy(memory.pressure)

  return (
    <div className="flex flex-wrap items-start justify-between gap-3 border-b pb-4">
      <div className="flex min-w-0 items-start gap-3">
        <MemoryStick className={cn("mt-0.5 size-4 shrink-0", copy.color)} />
        <div className="min-w-0">
          <div className={cn("text-sm font-medium", copy.color)}>
            {copy.label}
          </div>
          <div className="mt-0.5 text-xs text-muted-foreground">
            {copy.detail}
          </div>
        </div>
      </div>
      <div className="text-right text-xs leading-5 text-muted-foreground">
        <div>
          系统可用 {formatBytes(memory.availableBytes)} /{" "}
          {formatBytes(memory.totalBytes)}
        </div>
        <div>
          收缩线 {formatBytes(memory.shrinkingReserveBytes)} · 紧急线{" "}
          {formatBytes(memory.emergencyReserveBytes)} · 空闲 Agent 预算{" "}
          {formatBytes(memory.idleAgentBudgetBytes)}
        </div>
      </div>
    </div>
  )
}
