"use client"

import { useTranslations } from "next-intl"
import { Message, MessageContent } from "@/components/ai-elements/message"
import type { AgentType } from "@/lib/types"
import { useSessionActivity } from "@/hooks/use-session-activity"
import { useVisibleNow } from "@/hooks/use-visible-now"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { activityAge } from "@/lib/session-activity"
import { useConversationPresentation } from "@/components/chat/conversation-presentation-context"
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
}

export function PendingReplyIndicator({
  agentType,
  contextKey,
  startedAt,
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
          <StartupActivity contextKey={contextKey} startedAt={startedAt} />
        </div>
      </MessageContent>
    </Message>
  )
}

function StartupActivity({
  contextKey,
  startedAt,
}: Pick<PendingReplyIndicatorProps, "contextKey" | "startedAt">) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const activity = useSessionActivity(contextKey)
  const ref = useConversationActivityAnchor("activity")
  const { pageVisible, activityVisible } = useConversationActivityVisibility()
  const clock = useVisibleNow(pageVisible && activityVisible)
  const now = Math.max(clock, activity?.receivedAt ?? 0)
  // 与底部状态栏使用同一轮活动时间，组件重新挂载不能重置计时。
  const elapsed =
    activityAge(activity, activity?.snapshot.started_at, now) ??
    (startedAt === null ? null : Math.max(0, now - startedAt))
  return (
    <div
      ref={ref}
      className="conversation-current-activity"
      data-paused={!pageVisible || !activityVisible}
    >
      <ConversationWorkingMark />
      <ConversationActivityText
        phase={t("awaitingModelResponse")}
        detail={t("detail.awaitingModelResponse")}
        elapsed={
          elapsed === null
            ? null
            : t("elapsed", { elapsed: formatElapsedLabel(elapsed, t) })
        }
        waiting
        visible={activityVisible}
      />
    </div>
  )
}
