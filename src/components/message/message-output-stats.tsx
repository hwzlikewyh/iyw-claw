"use client"

import { memo } from "react"
import { useTranslations } from "next-intl"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import {
  firstTokenElapsed,
  recordedFirstTokenTime,
} from "@/lib/turn-performance"

interface MessageOutputStatsProps {
  messageId: string
  outputTokens?: number | null
  durationMs?: number | null
  liveMessage?: LiveMessage | null
  toolCallCount: number
  isStreaming: boolean
}

const MILLISECONDS_PER_SECOND = 1_000

export const MessageOutputStats = memo(function MessageOutputStats(
  props: MessageOutputStatsProps
) {
  const { outputTokens, durationMs, isStreaming, toolCallCount } = props
  const firstTokenMs = props.liveMessage
    ? firstTokenElapsed(props.liveMessage)
    : recordedFirstTokenTime(props.messageId)
  const t = useTranslations("Folder.chat.liveTurnStats")
  const hasRate =
    !isStreaming &&
    outputTokens != null &&
    Number.isFinite(outputTokens) &&
    outputTokens >= 0 &&
    durationMs != null &&
    Number.isFinite(durationMs) &&
    durationMs > 0
  const rate = hasRate
    ? (outputTokens / (durationMs / MILLISECONDS_PER_SECOND)).toFixed(1)
    : "--"
  return (
    <OutputStatsView
      rate={rate}
      firstToken={
        firstTokenMs == null
          ? t(isStreaming ? "metricsPending" : "metricsUnavailable")
          : `${(firstTokenMs / MILLISECONDS_PER_SECOND).toFixed(2)} s`
      }
      toolCallCount={toolCallCount}
    />
  )
})

function OutputStatsView({
  rate,
  firstToken,
  toolCallCount,
}: {
  rate: string
  firstToken: string
  toolCallCount: number
}) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  return (
    <div className="mt-3 flex min-h-5 flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground tabular-nums">
      <span>{t("outputRate", { rate })}</span>
      <span>{t("firstToken", { time: firstToken })}</span>
      <span>{t("toolUseCount", { count: toolCallCount })}</span>
    </div>
  )
}
