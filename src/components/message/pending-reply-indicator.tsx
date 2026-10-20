"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Message, MessageContent } from "@/components/ai-elements/message"
import type { AgentType } from "@/lib/types"
import { useVisibleNow } from "@/hooks/use-visible-now"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { useConversationPresentation } from "@/components/chat/conversation-presentation-context"
import { AssistantIdentity } from "./assistant-turn-status"
import { ConversationActivityText } from "./conversation-activity-text"
import { ConversationWorkingMark } from "./conversation-working-mark"
import {
  useConversationActivityAnchor,
  useConversationActivityVisibility,
} from "./conversation-activity-visibility"

export function PendingReplyIndicator({ agentType }: { agentType: AgentType }) {
  const state = useConversationPresentation()
  const [startedAt] = useState(Date.now)
  if (
    state?.awaitingInput ||
    state?.status === "disconnected" ||
    state?.status === "error"
  )
    return null
  const phase =
    state?.status === "connecting"
      ? "initializingConversation"
      : state?.status === "connected"
        ? "submittingConversation"
        : "awaitingModelResponse"
  return (
    <Message from="assistant">
      <MessageContent>
        <div className="conversation-assistant-header">
          <AssistantIdentity agentType={agentType} />
          <StartupActivity phase={phase} startedAt={startedAt} />
        </div>
      </MessageContent>
    </Message>
  )
}

function StartupActivity({
  phase,
  startedAt,
}: {
  phase:
    | "initializingConversation"
    | "submittingConversation"
    | "awaitingModelResponse"
  startedAt: number
}) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const ref = useConversationActivityAnchor("activity")
  const { pageVisible, activityVisible } = useConversationActivityVisibility()
  const now = useVisibleNow(pageVisible && activityVisible)
  return (
    <div
      ref={ref}
      className="conversation-current-activity"
      data-paused={!pageVisible || !activityVisible}
    >
      <ConversationWorkingMark />
      <ConversationActivityText
        phase={t(phase)}
        detail={t(`detail.${phase}`)}
        elapsed={t("elapsed", {
          elapsed: formatElapsedLabel(Math.max(0, now - startedAt), t),
        })}
        waiting
        visible={activityVisible}
      />
    </div>
  )
}
