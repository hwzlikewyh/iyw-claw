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
