import type { LiveMessage } from "@/contexts/acp-connections-context"

export const BACKGROUND_TASK_MARKER = "[[codeg-background-task]]"

export interface BackgroundTaskLifecycle {
  taskId: string
  status: string | null
  summary: string | null
  result: string | null
}

export function parseBackgroundTaskMarker(
  output: string | null | undefined
): BackgroundTaskLifecycle | null {
  if (!output) return null
  const trimmed = output.trimStart()
  if (!trimmed.startsWith(BACKGROUND_TASK_MARKER)) return null
  try {
    const payload = JSON.parse(
      trimmed.slice(BACKGROUND_TASK_MARKER.length)
    ) as Record<string, unknown>
    const taskId = typeof payload.task_id === "string" ? payload.task_id : null
    if (!taskId) return null
    return {
      taskId,
      status: typeof payload.status === "string" ? payload.status : null,
      summary: typeof payload.summary === "string" ? payload.summary : null,
      result: typeof payload.result === "string" ? payload.result : null,
    }
  } catch {
    return null
  }
}

export function isAsyncLaunchAckText(
  output: string | null | undefined
): boolean {
  return output?.includes("Async agent launched successfully") ?? false
}

export function settleLiveBackgroundTask(
  live: LiveMessage | null,
  settlement: BackgroundTaskLifecycle & { toolUseId: string }
): LiveMessage | null {
  if (!live) return live
  const output =
    BACKGROUND_TASK_MARKER +
    JSON.stringify({
      task_id: settlement.taskId,
      status: settlement.status,
      summary: settlement.summary,
      result: settlement.result,
    })
  let changed = false
  const content = live.content.map((block) => {
    if (
      block.type !== "tool_call" ||
      block.info.tool_call_id !== settlement.toolUseId
    )
      return block
    changed = true
    return {
      ...block,
      info: {
        ...block.info,
        status: "completed",
        raw_output_chunks: [output],
        raw_output_total_bytes: new TextEncoder().encode(output).length,
      },
    }
  })
  return changed ? { ...live, content } : live
}
