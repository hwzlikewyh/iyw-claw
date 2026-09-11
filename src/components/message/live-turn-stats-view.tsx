"use client"

import { memo, useMemo, type ReactNode } from "react"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { AgentType, PlanEntryInfo } from "@/lib/types"
import { LiveTurnStatusRow } from "./live-turn-status-row"
import { useTurnActivity } from "./live-turn-activity"
import { summarizeLiveTurn } from "./live-turn-summary"

interface LiveTurnStatsProps {
  agentType: AgentType
  contextKey?: string
  message: LiveMessage | null
  modelName?: string | null
  isStreaming?: boolean
  planEntries?: PlanEntryInfo[] | null
  subAgentControl?: ReactNode
  trailingStatus?: ReactNode
  onCancel?: () => void
  isAwaitingUserInput?: boolean
}

export const LiveTurnStats = memo(function LiveTurnStats({
  agentType,
  contextKey,
  message,
  isStreaming = true,
  planEntries,
  subAgentControl,
  trailingStatus,
  onCancel,
  isAwaitingUserInput,
}: LiveTurnStatsProps) {
  const summary = useMemo(() => summarizeLiveTurn(message), [message])
  const resolvedSummary = useMemo(
    () => (planEntries ? { ...summary, planEntries } : summary),
    [summary, planEntries]
  )
  const activity = useTurnActivity(contextKey, {
    summary: resolvedSummary,
    awaitingUser: isAwaitingUserInput,
  })
  return (
    <LiveTurnStatusRow
      agentType={agentType}
      phase={activity.phase}
      detail={activity.detail}
      icon={activity.icon}
      waiting={activity.waiting}
      attention={activity.attention}
      startedAt={message?.startedAt ?? null}
      planEntries={resolvedSummary.planEntries}
      isStreaming={isStreaming}
      subAgentControl={subAgentControl}
      trailingStatus={trailingStatus}
      onCancel={onCancel}
    />
  )
})
