import type { ProcessObservation, SessionActivitySnapshot } from "@/lib/types"

export interface ObservedSessionActivity {
  snapshot: SessionActivitySnapshot
  receivedAt: number
  restored: boolean
}

export function observeSessionActivity(
  snapshot: SessionActivitySnapshot | null | undefined,
  restored = false
): ObservedSessionActivity | null {
  if (!snapshot || !Number.isFinite(Date.parse(snapshot.sampled_at)))
    return null
  return { snapshot, receivedAt: Date.now(), restored }
}

export function activityTimings(
  activity: ObservedSessionActivity | null,
  input: { now: number; hasActiveTool: boolean }
) {
  const data = activity?.snapshot
  const process = latestObservedProcess(
    (data?.processes ?? []).filter(
      (p) =>
        p.status === "running" && p.turn_generation === data?.turn_generation
    )
  )
  const toolWaiting = input.hasActiveTool || process !== null
  const latestOutput = latestTimestamp([
    data?.text_at,
    data?.thinking_at,
    data?.tool_output_at,
    data?.started_at,
  ])
  return {
    process,
    toolWaiting,
    silence: activityAge(
      activity,
      toolWaiting
        ? latestTimestamp([
            data?.tool_output_at,
            data?.tool_started_at,
            data?.started_at,
          ])
        : latestOutput,
      input.now
    ),
    textAge: activityAge(activity, data?.text_at, input.now),
    thinkingAge: activityAge(activity, data?.thinking_at, input.now),
    pollAge: activityAge(activity, process?.checked_at, input.now),
  }
}

export function latestObservedProcess(
  processes: ProcessObservation[]
): ProcessObservation | null {
  return processes.reduce<ProcessObservation | null>(
    (latest, process) =>
      !latest || Date.parse(process.checked_at) > Date.parse(latest.checked_at)
        ? process
        : latest,
    null
  )
}

function latestTimestamp(values: (string | null | undefined)[]): string | null {
  return values.reduce<string | null>(
    (latest, at) =>
      at && (!latest || Date.parse(at) > Date.parse(latest)) ? at : latest,
    null
  )
}

// 使用服务端采样时间与本地接收时间对齐，避免远程主机时钟偏差。
export function activityAge(
  activity: ObservedSessionActivity | null,
  timestamp: string | null | undefined,
  now: number
): number | null {
  if (!activity || !timestamp) return null
  const sampledAt = Date.parse(activity.snapshot.sampled_at)
  const at = Date.parse(timestamp)
  if (!Number.isFinite(at)) return null
  return Math.max(0, sampledAt - at) + Math.max(0, now - activity.receivedAt)
}
