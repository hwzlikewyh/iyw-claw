"use client"

import { memo, useEffect } from "react"
import { useTranslations } from "next-intl"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { TurnUsage } from "@/lib/types"
import { TurnUsageStats } from "./turn-usage-stats"
import { useLiveTurnUsage } from "./use-live-turn-usage"
import { TooltipProvider } from "@/components/ui/tooltip"
import { resolveTurnDuration } from "@/lib/turn-duration"
import { useRecentOutputRate } from "@/hooks/use-recent-output-rate"
import { resolveUsagePoints } from "@/lib/usage-points"
import {
  measuredTurnTimings,
  recordedTurnTimings,
  rememberTurnTimings,
  type TurnTimings,
} from "@/lib/turn-performance"

interface MessageOutputStatsProps {
  messageId: string
  conversationId: number | null
  usage?: TurnUsage | null
  durationMs?: number | null
  completedAt?: string | null
  liveMessage?: LiveMessage | null
  toolCallCount: number | null
  outputCharacters?: number
  isStreaming: boolean
}

const MILLISECONDS_PER_SECOND = 1_000

function formatRate(value: number | null) {
  if (value == null || !Number.isFinite(value) || value <= 0) return null
  const rate = value.toFixed(1)
  return Number(rate) > 0 ? rate : null
}

function formatOutputRate(
  outputTokens?: number | null,
  durationMs?: number | null
) {
  if (
    outputTokens == null ||
    !Number.isFinite(outputTokens) ||
    outputTokens <= 0 ||
    durationMs == null ||
    !Number.isFinite(durationMs) ||
    durationMs <= 0
  )
    return null
  return formatRate(outputTokens / (durationMs / MILLISECONDS_PER_SECOND))
}

function completedDuration(props: MessageOutputStatsProps, startedAt?: number) {
  return resolveTurnDuration({
    duration_ms: props.durationMs,
    startedAt: startedAt == null ? null : new Date(startedAt).toISOString(),
    completedAt: props.completedAt,
  })
}

function useMeasuredTurnTimings(props: MessageOutputStatsProps) {
  const timings = props.liveMessage
    ? measuredTurnTimings(props.liveMessage)
    : recordedTurnTimings(props.messageId)
  const { firstActivityMs, firstThinkingMs, firstTextMs } = timings ?? {}
  useEffect(() => {
    if (
      firstActivityMs == null &&
      firstThinkingMs == null &&
      firstTextMs == null
    )
      return
    rememberTurnTimings(props.messageId, {
      firstActivityMs: firstActivityMs ?? null,
      firstThinkingMs: firstThinkingMs ?? null,
      firstTextMs: firstTextMs ?? null,
    })
  }, [props.messageId, firstActivityMs, firstThinkingMs, firstTextMs])
  return timings
}

function useOutputUsage(props: MessageOutputStatsProps) {
  const liveUsage = useLiveTurnUsage({
    conversationId: props.conversationId,
    messageId: props.messageId,
    enabled: props.isStreaming && Boolean(props.liveMessage),
    startedAt: props.liveMessage?.startedAt ?? null,
  })
  const usage = props.isStreaming
    ? (liveUsage?.usage ?? props.usage)
    : (props.usage ?? liveUsage?.usage)
  return { usage, startedAt: liveUsage?.startedAt }
}

export const MessageOutputStats = memo(function MessageOutputStats(
  props: MessageOutputStatsProps
) {
  const { isStreaming } = props
  const { usage, startedAt } = useOutputUsage(props)
  const durationMs = completedDuration(props, startedAt)
  const recentOutputRate = useRecentOutputRate(
    isStreaming ? (props.liveMessage?.id ?? null) : null,
    usage?.output_tokens ?? null
  )
  const timings = useMeasuredTurnTimings(props)
  return (
    <OutputStatsView
      rate={
        isStreaming
          ? formatRate(recentOutputRate)
          : formatOutputRate(usage?.output_tokens, durationMs)
      }
      timings={timings}
      toolCallCount={props.toolCallCount}
      usage={usage}
      outputCharacters={props.outputCharacters}
      isStreaming={isStreaming}
    />
  )
})

function validTiming(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
}

function positiveCount(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
}

function useTurnTimingStats(timings: TurnTimings | null) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const fields = [
    ["firstActivityMs", "firstActivity"],
    ["firstThinkingMs", "firstThinking"],
    ["firstTextMs", "firstToken"],
  ] as const
  return fields.map(([key, label]) => {
    const value = timings?.[key]
    return validTiming(value) ? (
      <span key={key}>
        {t(label, {
          time: `${(value / MILLISECONDS_PER_SECOND).toFixed(2)} s`,
        })}
      </span>
    ) : null
  })
}

interface OutputStatsViewProps {
  rate: string | null
  timings: TurnTimings | null
  toolCallCount: number | null
  usage?: TurnUsage | null
  isStreaming: boolean
  outputCharacters?: number
}

function hasUsagePoints(usage?: TurnUsage | null): usage is TurnUsage {
  return (
    usage != null &&
    resolveUsagePoints(
      usage.confirmed_points?.amount,
      usage.estimated_points
    ) !== null
  )
}

function OutputStatsView({
  rate,
  timings,
  toolCallCount,
  usage,
  isStreaming,
  outputCharacters,
}: OutputStatsViewProps) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const bodyT = useTranslations("ConversationPresentation")
  const timingStats = useTurnTimingStats(timings)
  const stats = [
    rate && (
      <span
        key="rate"
        title={t(isStreaming ? "recentOutputRateHint" : "outputRateHint")}
      >
        {t("outputRate", { rate })}
      </span>
    ),
    ...timingStats,
    positiveCount(outputCharacters) && (
      <span key="characters">
        {bodyT("bodyCharacters", { count: outputCharacters })}
      </span>
    ),
    positiveCount(toolCallCount) && (
      <span key="tools">{t("toolUseCount", { count: toolCallCount })}</span>
    ),
    hasUsagePoints(usage) && (
      <TooltipProvider key="usage" delayDuration={150}>
        <TurnUsageStats usage={usage} />
      </TooltipProvider>
    ),
  ].filter(Boolean)
  if (stats.length === 0) return null
  return (
    <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground tabular-nums">
      {stats}
    </div>
  )
}
