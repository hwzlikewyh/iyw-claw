import { isTerminalTask, type RuntimeTask } from "@/lib/runtime-tasks"

export function isTaskHistory(task: RuntimeTask) {
  return (
    isTerminalTask(task.status) ||
    ["expired", "declined", "canceled"].includes(task.status)
  )
}

export function isListeningTask(task: RuntimeTask) {
  return (
    task.kind === "monitor" &&
    ["active", "running", "waiting"].includes(task.status)
  )
}

export function taskControlsAvailable(
  status: string | null,
  error: string | null
) {
  return !error && (status === "connected" || status === "prompting")
}
