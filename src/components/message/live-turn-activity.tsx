"use client"

import { useTranslations } from "next-intl"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { OUTPUT_RATE_WINDOW_MS } from "@/hooks/use-recent-output-rate"
import { activityTimings } from "@/lib/session-activity"
import { formatElapsedLabel } from "@/lib/format-elapsed"

const SILENCE_WARNING_MS = 5 * 60_000

interface TurnActivityInput {
  message: LiveMessage | null
  now: number
  awaitingUser?: boolean
  recentRate: number | null
}

export function useTurnActivity(
  contextKey: string | undefined,
  input: TurnActivityInput
) {
  const activity = useSessionActivity(contextKey)
  const t = useTranslations("Folder.chat.liveTurnStats")
  const observation = turnObservation(activity, input)
  const { silence, toolWaiting, pollAge } = observation.timing
  const warning =
    !input.awaitingUser && silence !== null && silence >= SILENCE_WARNING_MS
  const phase = selectPhase(observation)
  const ageLabel = (age: number) => formatElapsedLabel(age, t)
  const notice =
    silence === null
      ? null
      : t(toolWaiting ? "toolSilence" : "outputSilence", {
          elapsed: ageLabel(silence),
        })
  return {
    phase: phase === null ? null : t(phase, { model: "原助理" }),
    warning,
    waiting:
      warning ||
      (phase !== null && phase !== "streaming" && phase !== "thinking"),
    notice,
    processNotice:
      pollAge === null
        ? null
        : t("processObserved", { elapsed: ageLabel(pollAge) }),
  }
}

function turnObservation(
  activity: ReturnType<typeof useSessionActivity>,
  input: TurnActivityInput
) {
  const hasActiveTool =
    input.message?.content.some(
      (block) =>
        block.type === "tool_call" &&
        ["in_progress", "inprogress", "pending"].includes(block.info.status)
    ) ?? false
  return {
    timing: activityTimings(activity, { now: input.now, hasActiveTool }),
    hasActiveTool,
    retrying: Boolean(activity?.snapshot.retrying_since),
    known: Boolean(activity),
    recentRate: input.recentRate,
  }
}

function selectPhase(input: {
  timing: ReturnType<typeof activityTimings>
  hasActiveTool: boolean
  retrying: boolean
  known: boolean
  recentRate: number | null
}) {
  if (input.retrying) return "runtimeRetrying"
  const { textAge, thinkingAge, process } = input.timing
  if (
    (textAge !== null && textAge < OUTPUT_RATE_WINDOW_MS) ||
    (!input.known && (input.recentRate ?? 0) > 0)
  )
    return "streaming"
  if (process) return "waitingProcess"
  if (input.hasActiveTool) return null
  if (thinkingAge !== null && thinkingAge < OUTPUT_RATE_WINDOW_MS)
    return "thinking"
  return input.known ? "waitingOutput" : "activityUnknown"
}
