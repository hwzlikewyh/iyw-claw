"use client"

import { useMemo } from "react"
import { Message, MessageContent } from "@/components/ai-elements/message"
import type { AgentType } from "@/lib/types"
import type { LiveMessage } from "@/contexts/acp-connections-context"
import { useConversationPresentation } from "@/components/chat/conversation-presentation-context"
import { useTurnActivity } from "./live-turn-activity"
import { summarizeLiveTurn } from "./live-turn-summary"
import { AssistantIdentity } from "./assistant-turn-status"
import { ConversationActivityText } from "./conversation-activity-text"
import { ConversationWorkingMark } from "./conversation-working-mark"
import {
  useConversationActivityAnchor,
  useConversationActivityVisibility,
} from "./conversation-activity-visibility"

interface PendingReplyIndicatorProps {
  agentType: AgentType
  contextKey?: string
  startedAt: number | null
  message: LiveMessage | null
}

export function PendingReplyIndicator({
  agentType,
  contextKey,
  startedAt,
  message,
}: PendingReplyIndicatorProps) {
  const state = useConversationPresentation()
  if (
    state?.awaitingInput ||
    state?.status === "connecting" ||
    state?.status === "connected" ||
    state?.status === "disconnected" ||
    state?.status === "error"
  )
    return null
  return (
    <Message from="assistant">
      <MessageContent>
        <div className="conversation-assistant-header">
          <AssistantIdentity agentType={agentType} />
          <StartupActivity
            contextKey={contextKey}
            startedAt={startedAt}
            message={message}
          />
        </div>
      </MessageContent>
    </Message>
  )
}

function StartupActivity({
  contextKey,
  startedAt,
  message,
}: Pick<PendingReplyIndicatorProps, "contextKey" | "startedAt" | "message">) {
  const summary = useMemo(() => summarizeLiveTurn(message), [message])
  const ref = useConversationActivityAnchor("activity")
  const { pageVisible, activityVisible } = useConversationActivityVisibility()
  const activity = useTurnActivity(contextKey, {
    summary,
    startedAt,
    visible: pageVisible && activityVisible,
  })
  return (
    <div
      ref={ref}
      className="conversation-current-activity"
      data-paused={!pageVisible || !activityVisible}
    >
      <ConversationWorkingMark />
      <ConversationActivityText {...activity} visible={activityVisible} />
    </div>
  )
}
