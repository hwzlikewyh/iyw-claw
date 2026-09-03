"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"

import type { LiveMessage } from "@/contexts/acp-connections-context"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import type { PlanEntryInfo } from "@/lib/types"
import { LiveTurnStatusRow } from "@/components/message/live-turn-status-row"

interface LiveTurnStatsProps {
  message: LiveMessage | null
  modelName?: string | null
  isStreaming?: boolean
  planEntries?: PlanEntryInfo[] | null
  subAgentControl?: ReactNode
  trailingStatus?: ReactNode
  onCancel?: () => void
  isAwaitingUserInput?: boolean
}

const EMPTY_PLAN_ENTRIES: PlanEntryInfo[] = []

function getLatestPlanEntries(message: LiveMessage | null): PlanEntryInfo[] {
  if (!message) return EMPTY_PLAN_ENTRIES
  for (let index = message.content.length - 1; index >= 0; index -= 1) {
    const block = message.content[index]
    if (block.type === "plan") return block.entries
  }
  return EMPTY_PLAN_ENTRIES
}

function countToolCalls(message: LiveMessage | null): number {
  return (message?.content ?? []).filter((block) => block.type === "tool_call")
    .length
}

function countOutputCharacters(message: LiveMessage | null): number {
  return (message?.content ?? []).reduce(
    (count, block) =>
      block.type === "text" ? count + Array.from(block.text).length : count,
    0
  )
}

function useElapsed(startedAt: number | null): [number, number] {
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    if (startedAt === null) return
    const timer = setInterval(() => {
      setNow(Date.now())
    }, 1_000)
    return () => clearInterval(timer)
  }, [startedAt])
  return [now, startedAt === null ? 0 : Math.max(0, now - startedAt)]
}

export function LiveTurnStats({
  message,
  modelName,
  isStreaming = true,
  planEntries,
  subAgentControl,
  trailingStatus,
  onCancel,
  isAwaitingUserInput,
}: LiveTurnStatsProps) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const startedAt = message?.startedAt ?? null
  const [now, elapsed] = useElapsed(startedAt)
  const outputRateLabel = useMemo(() => {
    const outputCharacters = countOutputCharacters(message)
    if (outputCharacters === 0) return null
    // Older snapshots do not carry firstTextAt; startedAt is a conservative
    // fallback so a refreshed in-progress turn still exposes a useful rate.
    const firstTextAt = message?.firstTextAt ?? startedAt
    if (firstTextAt === null) return null
    const seconds = Math.max(1, (now - firstTextAt) / 1_000)
    return t("outputRate", {
      rate: Math.round(outputCharacters / seconds),
    })
  }, [message, now, startedAt, t])
  const resolvedPlanEntries = useMemo(
    () => planEntries ?? getLatestPlanEntries(message),
    [message, planEntries]
  )
  const completedPlanCount = useMemo(
    () =>
      resolvedPlanEntries.filter((entry) => entry.status === "completed")
        .length,
    [resolvedPlanEntries]
  )
  const elapsedLabel = message ? formatElapsedLabel(elapsed, t) : null

  return (
    <LiveTurnStatusRow
      message={message}
      modelName={modelName}
      isStreaming={isStreaming}
      planEntries={resolvedPlanEntries}
      completedPlanCount={completedPlanCount}
      elapsedLabel={elapsedLabel}
      outputRateLabel={outputRateLabel}
      toolCallCount={countToolCalls(message)}
      subAgentControl={subAgentControl}
      trailingStatus={trailingStatus}
      onCancel={onCancel}
      isAwaitingUserInput={isAwaitingUserInput}
    />
  )
}
