import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { hasProcessError } from "./assistant-turn-process"
import { isCommandLikeTool } from "@/lib/command-display"
import { commandExitCode } from "./command-output-presentation"
import { buildBackgroundTaskRows } from "@/lib/background-task"
import { buildDelegationTaskRows } from "@/lib/delegation-status"
import { isContextCompactionMeta } from "@/lib/context-compaction"

export function processPartHasFailure(part: AdaptedContentPart): boolean {
  if (hasProcessError(part)) return true
  if (
    part.type === "tool-call" &&
    isCommandLikeTool(part.toolName, part.input)
  ) {
    const exitCode = commandExitCode(part.output)
    return exitCode !== null && exitCode !== 0
  }
  if (part.type === "tool-group") return part.items.some(processPartHasFailure)
  if (part.type === "goal-run")
    return (
      processPartHasFailure(part.start) ||
      Boolean(part.end && processPartHasFailure(part.end)) ||
      part.items.some(processPartHasFailure)
    )
  if (part.type === "background-task-group")
    return buildBackgroundTaskRows(part.polls).some(
      (row) => row.badge === "failed"
    )
  if (part.type === "delegation-status-group")
    return buildDelegationTaskRows(part.polls).some(
      (row) => row.badge.status === "err"
    )
  return false
}

export function processPartHasCompaction(part: AdaptedContentPart): boolean {
  if (part.type === "tool-call") return isContextCompactionMeta(part.meta)
  if (part.type === "tool-group")
    return part.items.some(processPartHasCompaction)
  if (part.type === "goal-run") return part.items.some(processPartHasCompaction)
  return false
}

const LIVE_TAIL_PARTS = 2
export const LIVE_TEXT_MAX_HEIGHT = 240

export function processPartNeedsAttention(part: AdaptedContentPart): boolean {
  if (processPartHasFailure(part)) return true
  if (part.type === "tool-call") return part.state.startsWith("input-")
  if (part.type === "tool-group")
    return part.items.some(processPartNeedsAttention)
  if (part.type === "reasoning") return part.isStreaming
  if (part.type === "goal-run") return part.isRunning
  // 后台任务的工具回执不代表进程结束，复用状态卡片的真实判定。
  if (part.type === "background-task-group")
    return buildBackgroundTaskRows(part.polls).some(
      (row) => row.badge !== "completed" && row.badge !== "stopped"
    )
  if (part.type === "delegation-status-group")
    return buildDelegationTaskRows(part.polls).some(
      (row) => row.badge.status !== "ok"
    )
  return false
}

export function processTailStart(parts: AdaptedContentPart[]): number {
  return Math.max(0, parts.length - LIVE_TAIL_PARTS)
}
