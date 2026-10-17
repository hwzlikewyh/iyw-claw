import { getTransport } from "@/lib/transport"

export type RuntimeTaskAction = "pause" | "resume" | "clear" | "stop"

export interface RuntimeTask {
  id: string
  kind: "goal" | "loop" | "subagent" | "background" | "monitor" | "delegation"
  native_id: string
  title: string
  status: string
  actions: RuntimeTaskAction[]
  parent_id: string | null
  tool_call_id: string | null
  objective: string | null
  summary: string | null
  output: string | null
  output_file: string | null
  model: string | null
  schedule: string | null
  next_run: string | null
  updated_at: string
  child_conversation_id: number | null
  child_connection_id: string | null
  agent_type: import("@/lib/types").AgentType | null
  transcript_id?: string | null
  background?: {
    task_type: string
    command: string | null
    timeout_ms: number | null
    persistent: boolean | null
    ambient: boolean
  } | null
}

export interface RuntimeTaskActivity {
  outstanding: number
  monitors: number
  uncertain: boolean
}

export function runtimeBackgroundState(connection: {
  status?: string | null
  backgroundOutstanding: number
  backgroundUncertain: boolean
  runtimeTaskActivity?: RuntimeTaskActivity | null
}) {
  const activity = connection.runtimeTaskActivity
  return {
    outstanding: Math.max(
      connection.backgroundOutstanding,
      activity?.outstanding ?? 0
    ),
    monitors: activity?.monitors ?? 0,
    uncertain:
      connection.backgroundUncertain ||
      activity?.uncertain === true ||
      ((connection.status === "disconnected" ||
        connection.status === "error") &&
        (activity?.outstanding ?? 0) > 0),
  }
}

export interface RuntimeTaskSnapshot {
  tasks: RuntimeTask[]
  revision: number
}

export function loadRuntimeTasks(
  connectionId: string | null,
  conversationId: number | null
) {
  return getTransport().call<RuntimeTaskSnapshot>("acp_runtime_tasks", {
    connectionId,
    conversationId,
  })
}

export function controlRuntimeTask(
  connectionId: string,
  request: { task_id: string; native_id: string; action: string }
) {
  return getTransport().call<RuntimeTaskSnapshot>("acp_control_runtime_task", {
    connectionId,
    request,
  })
}

export function isTerminalTask(status: string) {
  return [
    "completed",
    "complete",
    "failed",
    "stopped",
    "cancelled",
    "cleared",
    "budget_limited",
    "budgetLimited",
  ].includes(status)
}

export function loadRuntimeTaskDetail(connectionId: string, taskId: string) {
  return getTransport().call<import("./types").ConversationDetail>(
    "acp_runtime_task_detail",
    { connectionId, taskId }
  )
}
