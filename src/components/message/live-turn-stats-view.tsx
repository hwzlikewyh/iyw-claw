"use client"

import { useEffect, useMemo, useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"

import type {
  LiveContentBlock,
  LiveMessage,
} from "@/contexts/acp-connections-context"
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
const textCharacterCounts = new WeakMap<LiveContentBlock, number>()
const toolCallCounts = new WeakMap<LiveMessage, number>()

function countTextCharacters(block: LiveContentBlock): number {
  if (block.type !== "text") return 0
  const cached = textCharacterCounts.get(block)
  if (cached !== undefined) return cached
  let count = 0
  // 按 Unicode 码点计数，保持原有 Array.from 的统计口径。
  for (const character of block.text) {
    if (character) count += 1
  }
  textCharacterCounts.set(block, count)
  return count
}

function getLatestPlanEntries(message: LiveMessage | null): PlanEntryInfo[] {
  if (!message) return EMPTY_PLAN_ENTRIES
  for (let index = message.content.length - 1; index >= 0; index -= 1) {
    const block = message.content[index]
    if (block.type === "plan") return block.entries
  }
  return EMPTY_PLAN_ENTRIES
}

function countToolCalls(message: LiveMessage | null): number {
  if (!message) return 0
  const cached = toolCallCounts.get(message)
  if (cached !== undefined) return cached
  let count = 0
  for (const block of message.content) {
    if (block.type === "tool_call") count += 1
  }
  toolCallCounts.set(message, count)
  return count
}

function countOutputCharacters(message: LiveMessage | null): number {
  let count = 0
  for (const block of message?.content ?? []) {
    count += countTextCharacters(block)
  }
  return count
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
  const outputCharacters = useMemo(
    () => countOutputCharacters(message),
    [message]
  )
  const outputRateLabel = useMemo(() => {
    if (outputCharacters === 0) return null
    // Older snapshots do not carry firstTextAt; startedAt is a conservative
    // fallback so a refreshed in-progress turn still exposes a useful rate.
    const firstTextAt = message?.firstTextAt ?? startedAt
    if (firstTextAt === null) return null
    const seconds = Math.max(1, (now - firstTextAt) / 1_000)
    return t("outputRate", {
      rate: Math.round(outputCharacters / seconds),
    })
  }, [message?.firstTextAt, now, outputCharacters, startedAt, t])
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
