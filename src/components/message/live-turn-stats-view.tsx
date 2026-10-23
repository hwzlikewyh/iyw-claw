"use client"

import { memo, useMemo, type ReactNode } from "react"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { AgentType, PlanEntryInfo } from "@/lib/types"
import { LiveTurnStatusRow } from "./live-turn-status-row"
import { useTurnActivity } from "./live-turn-activity"
import { summarizeLiveTurn } from "./live-turn-summary"
import { useConversationActivityVisibility } from "./conversation-activity-visibility"

interface LiveTurnStatsProps {
  agentType: AgentType
  contextKey?: string
  message: LiveMessage | null
  parts?: readonly AdaptedContentPart[]
  modelName?: string | null
  isStreaming?: boolean
  planEntries?: PlanEntryInfo[] | null
  subAgentControl?: ReactNode
  trailingStatus?: ReactNode
  onCancel?: () => void
  onReturnToActivity?: () => void
  isAwaitingUserInput?: boolean
}

export const LiveTurnStats = memo(function LiveTurnStats({
  agentType,
  contextKey,
  message,
  parts,
  isStreaming = true,
  planEntries,
  subAgentControl,
  trailingStatus,
  onCancel,
  onReturnToActivity,
  isAwaitingUserInput,
}: LiveTurnStatsProps) {
  const summary = useMemo(() => summarizeLiveTurn(message), [message])
  const resolvedSummary = useMemo(
    () => (planEntries ? { ...summary, planEntries } : summary),
    [summary, planEntries]
  )
  const { pageVisible } = useConversationActivityVisibility()
  const activity = useTurnActivity(contextKey, {
    summary: resolvedSummary,
    awaitingUser: isAwaitingUserInput,
    parts,
    startedAt: message?.startedAt,
    visible: pageVisible,
  })
  return (
    <LiveTurnStatusRow
      contextKey={contextKey}
      agentType={agentType}
      phase={activity.phase}
      detail={activity.detail}
      elapsed={activity.elapsed}
      icon={activity.icon}
      waiting={activity.waiting}
      attention={activity.attention}
      startedAt={message?.startedAt ?? null}
      planEntries={resolvedSummary.planEntries}
      isStreaming={isStreaming}
      subAgentControl={subAgentControl}
      trailingStatus={trailingStatus}
      onCancel={onCancel}
      onReturnToActivity={onReturnToActivity}
    />
  )
})
