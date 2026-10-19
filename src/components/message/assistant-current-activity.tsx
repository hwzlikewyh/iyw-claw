"use client"

import { useMemo } from "react"
import { CircleDot, Radio } from "lucide-react"
import { useTranslations } from "next-intl"
import { useConversationPresentation } from "@/components/chat/conversation-presentation-context"
import { RuntimeTaskPopover } from "@/components/chat/runtime-task-panel"
import { isListeningTask } from "@/components/chat/runtime-task-presentation"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import {
  useConversationActivityAnchor,
  useConversationActivityVisibility,
} from "./conversation-activity-visibility"
import { useTurnActivity } from "./live-turn-activity"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"
import { summarizeLiveTurn } from "./live-turn-summary"

export function AssistantCurrentActivity({
  conversationId,
}: {
  conversationId: number
}) {
  const state = useConversationPresentation()
  if (
    !state ||
    state.conversationId !== conversationId ||
    state.status !== "prompting" ||
    state.awaitingInput
  )
    return null
  return (
    <CurrentActivity
      contextKey={state.contextKey}
      conversationId={conversationId}
    />
  )
}

function CurrentActivity({
  contextKey,
  conversationId,
}: {
  contextKey?: string
  conversationId: number
}) {
  const message = useConversationRuntimeStore(
    (store) => store.byConversationId.get(conversationId)?.liveMessage ?? null
  )
  const summary = useMemo(() => summarizeLiveTurn(message), [message])
  const activity = useTurnActivity(contextKey, { summary })
  const ref = useConversationActivityAnchor("activity")
  const { pageVisible, activityVisible } = useConversationActivityVisibility()
  return (
    <div
      ref={ref}
      className="conversation-current-activity"
      data-paused={!pageVisible || !activityVisible}
    >
      <CircleDot
        aria-hidden="true"
        className={
          "size-3 shrink-0 " +
          (activity.waiting ? "" : "conversation-working-mark")
        }
      />
      <span>
        <strong
          role="status"
          aria-live={activityVisible ? "polite" : "off"}
          aria-atomic="true"
        >
          {activity.phase}
        </strong>
        {activity.waiting && <small>{activity.detail}</small>}
      </span>
    </div>
  )
}

function toolCallIds(parts: AdaptedContentPart[]): string[] {
  return parts.flatMap((part) => {
    if (part.type === "tool-call" || part.type === "tool-result")
      return [part.toolCallId]
    if (part.type === "tool-group") return toolCallIds(part.items)
    if (part.type === "goal-run")
      return [part.start.toolCallId, ...toolCallIds(part.items)]
    return []
  })
}

export function AssistantMonitorStatus({
  parts,
  conversationId,
}: {
  parts: AdaptedContentPart[]
  conversationId: number
}) {
  const state = useConversationPresentation()
  const ids = useMemo(() => new Set(toolCallIds(parts)), [parts])
  if (
    !state ||
    state.conversationId !== conversationId ||
    state.status !== "connected" ||
    state.error ||
    !state.tasks.some(
      (task) =>
        isListeningTask(task) &&
        task.tool_call_id != null &&
        ids.has(task.tool_call_id)
    )
  )
    return null
  return (
    <MonitorStatus
      showHint={!parts.some((part) => part.type === "text" && part.text.trim())}
    />
  )
}

function MonitorStatus({ showHint }: { showHint: boolean }) {
  const t = useTranslations("ConversationPresentation")
  const ref = useConversationActivityAnchor("monitor")
  return (
    <div ref={ref} className="conversation-monitor-status">
      {showHint && <p>{t("monitorStarted")}</p>}
      <RuntimeTaskPopover monitorsOnly>
        <Radio aria-hidden="true" className="size-3" />
        {t("monitoring")}
      </RuntimeTaskPopover>
    </div>
  )
}
