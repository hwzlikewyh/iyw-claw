"use client"

import { memo, useEffect } from "react"
import { useTranslations } from "next-intl"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { TurnUsage } from "@/lib/types"
import { TurnUsageStats } from "./turn-usage-stats"
import { getLiveToolCallCount } from "./message-output-metrics"
import { useLiveTurnDuration, useLiveTurnUsage } from "./use-live-turn-usage"
import { TooltipProvider } from "@/components/ui/tooltip"
import { resolveTurnDuration } from "@/lib/turn-duration"
import {
  firstTokenElapsed,
  recordedFirstTokenTime,
  rememberFirstTokenTime,
} from "@/lib/turn-performance"

interface MessageOutputStatsProps {
  messageId: string
  conversationId: number | null
  usage?: TurnUsage | null
  durationMs?: number | null
  completedAt?: string | null
  liveMessage?: LiveMessage | null
  toolCallCount: number
  isStreaming: boolean
}

const MILLISECONDS_PER_SECOND = 1_000

function formatOutputRate(
  outputTokens?: number | null,
  durationMs?: number | null
) {
  if (
    outputTokens == null ||
    !Number.isFinite(outputTokens) ||
    outputTokens < 0 ||
    durationMs == null ||
    !Number.isFinite(durationMs) ||
    durationMs <= 0
  )
    return "--"
  return (outputTokens / (durationMs / MILLISECONDS_PER_SECOND)).toFixed(1)
}

function completedDuration(props: MessageOutputStatsProps, startedAt?: number) {
  return (
    resolveTurnDuration({
      startedAt: startedAt == null ? null : new Date(startedAt).toISOString(),
      completedAt: props.completedAt,
    }) ?? props.durationMs
  )
}

export const MessageOutputStats = memo(function MessageOutputStats(
  props: MessageOutputStatsProps
) {
  const { isStreaming } = props
  const liveUsage = useLiveTurnUsage({
    conversationId: props.conversationId,
    messageId: props.messageId,
    enabled: isStreaming && Boolean(props.liveMessage),
    startedAt: props.liveMessage?.startedAt ?? null,
  })
  const elapsed = useLiveTurnDuration(
    isStreaming ? (props.liveMessage?.startedAt ?? null) : null
  )
  const usage = isStreaming
    ? (liveUsage?.usage ?? props.usage)
    : (props.usage ?? liveUsage?.usage)
  const durationMs = isStreaming
    ? elapsed
    : completedDuration(props, liveUsage?.startedAt)
  const firstTokenMs = props.liveMessage
    ? firstTokenElapsed(props.liveMessage)
    : recordedFirstTokenTime(props.messageId)
  const t = useTranslations("Folder.chat.liveTurnStats")
  useEffect(() => {
    rememberFirstTokenTime(props.messageId, firstTokenMs)
  }, [props.messageId, firstTokenMs])
  return (
    <OutputStatsView
      rate={formatOutputRate(usage?.output_tokens, durationMs)}
      firstToken={
        firstTokenMs == null
          ? t(isStreaming ? "metricsPending" : "metricsUnavailable")
          : `${(firstTokenMs / MILLISECONDS_PER_SECOND).toFixed(2)} s`
      }
      toolCallCount={
        props.liveMessage
          ? getLiveToolCallCount(props.liveMessage)
          : props.toolCallCount
      }
      usage={usage}
      isStreaming={isStreaming}
      isReportedUsage={isStreaming || !props.usage}
    />
  )
})

function OutputStatsView({
  rate,
  firstToken,
  toolCallCount,
  usage,
  isStreaming,
  isReportedUsage,
}: {
  rate: string
  firstToken: string
  toolCallCount: number
  usage?: TurnUsage | null
  isStreaming: boolean
  isReportedUsage: boolean
}) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const pointsT = useTranslations("UsagePoints")
  return (
    <div className="mt-3 flex min-h-5 flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground tabular-nums">
      <span title={t("outputRateHint")}>{t("outputRate", { rate })}</span>
      <span>{t("firstToken", { time: firstToken })}</span>
      <span>{t("toolUseCount", { count: toolCallCount })}</span>
      {usage ? (
        <TooltipProvider delayDuration={150}>
          <TurnUsageStats usage={usage} />
        </TooltipProvider>
      ) : (
        <span>
          {pointsT("turn")}:{" "}
          {t(isStreaming ? "metricsPending" : "metricsUnavailable")}
        </span>
      )}
      {isReportedUsage && usage && <span>{t("reportedUsage")}</span>}
    </div>
  )
}
