"use client"

import { memo } from "react"
import { useTranslations } from "next-intl"
import { useRecentOutputRate } from "@/hooks/use-recent-output-rate"
import { useSessionActivity } from "@/hooks/use-session-activity"

interface MessageOutputStatsProps {
  messageKey: string
  contextKey?: string
  characters: number
  toolCallCount: number
  isStreaming: boolean
  recoveredVersion?: number
}

export const MessageOutputStats = memo(function MessageOutputStats({
  messageKey,
  contextKey,
  characters,
  toolCallCount,
  isStreaming,
  recoveredVersion = 0,
}: MessageOutputStatsProps) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const activity = useSessionActivity(isStreaming ? contextKey : undefined)
  const rateKey = isStreaming
    ? `${messageKey}:${recoveredVersion}:${activity?.restored ?? false}`
    : null
  const rate = useRecentOutputRate(rateKey, characters) ?? 0
  return (
    <div className="mt-3 flex min-h-5 flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground tabular-nums">
      <span>{t("outputRate", { rate })}</span>
      <span>{t("toolUseCount", { count: toolCallCount })}</span>
    </div>
  )
})
