import type { AdaptedToolCallPart } from "./adapters/ai-elements-adapter"
import type {
  BackgroundTaskBadge,
  BackgroundTaskEnvelope,
} from "./background-task"

const STOPPED = new Set([
  "stopped",
  "killed",
  "cancelled",
  "canceled",
  "interrupted",
])
const FAILED = new Set(["failed", "error", "errored", "timeout", "timed_out"])

export function terminalBackgroundBadge(
  status: string | null
): BackgroundTaskBadge | null {
  if (status === "completed") return "completed"
  if (STOPPED.has(status ?? "")) return "stopped"
  if (FAILED.has(status ?? "")) return "failed"
  return null
}

export function resolveBackgroundOutcome(
  entries: {
    poll: AdaptedToolCallPart
    envelope: BackgroundTaskEnvelope | null
  }[]
) {
  // 轮询请求结束不等于任务结束；确认的终态不能被空轮询重新激活。
  for (const { poll, envelope } of [...entries].reverse()) {
    if (
      envelope?.kind === "stop" &&
      (poll.state === "output-error" || poll.errorText?.trim())
    )
      continue
    const status =
      envelope?.kind === "stop" ? "stopped" : (envelope?.status ?? null)
    const terminal = terminalBackgroundBadge(status)
    if (terminal)
      return {
        badge:
          terminal === "completed" && envelope?.exitCode
            ? ("failed" as const)
            : terminal,
        exitCode: envelope?.exitCode ?? null,
      }
  }
  const latest = entries[entries.length - 1]
  const running = latest.envelope?.status === "running"
  const pending = ["input-available", "input-streaming"].includes(
    latest.poll.state
  )
  return {
    badge: (running || pending ? "running" : "unknown") as BackgroundTaskBadge,
    exitCode: latest.envelope?.exitCode ?? null,
  }
}
