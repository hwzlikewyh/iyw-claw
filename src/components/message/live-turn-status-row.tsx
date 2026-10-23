"use client"

import { memo, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { CircleDot, CornerUpLeft, LoaderCircle, Square } from "lucide-react"
import type { AgentType, PlanEntryInfo } from "@/lib/types"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type { ActivityIcon } from "./live-turn-activity"
import { LiveTurnPlan } from "./live-turn-plan"
import { useConversationActivityVisibility } from "./conversation-activity-visibility"
import { useConversationPresentation } from "@/components/chat/conversation-presentation-context"
import { isTaskHistory } from "@/components/chat/runtime-task-presentation"

interface LiveTurnStatusRowProps {
  agentType: AgentType
  contextKey?: string
  phase: string
  detail: string
  elapsed?: string | null
  icon: ActivityIcon
  waiting: boolean
  attention: boolean
  startedAt: number | null
  isStreaming: boolean
  planEntries: PlanEntryInfo[]
  subAgentControl?: ReactNode
  trailingStatus?: ReactNode
  onCancel?: () => void
  onReturnToActivity?: () => void
}

function StopTurnButton({ onCancel }: { onCancel?: () => void }) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  if (!onCancel) return null
  return (
    <TooltipProvider delayDuration={150}>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            onClick={onCancel}
            aria-label={t("stop")}
            className="ms-auto inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Square aria-hidden="true" className="size-3 fill-current" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">{t("stop")}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

function TurnActivity({
  phase,
  detail,
  waiting,
  attention,
  elapsed,
}: Pick<
  LiveTurnStatusRowProps,
  "phase" | "detail" | "waiting" | "attention" | "elapsed"
>) {
  const Icon = attention ? CircleDot : LoaderCircle
  return (
    <span
      className="conversation-live-activity flex min-w-0 items-center gap-2 text-xs text-muted-foreground"
      data-attention={attention}
    >
      <Icon
        aria-hidden="true"
        className={
          "size-3 shrink-0 " + (attention ? "" : "conversation-working-mark")
        }
      />
      <span
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className={
          waiting && !attention
            ? "conversation-live-phase conversation-waiting-label"
            : "conversation-live-phase"
        }
      >
        {phase}
      </span>
      {elapsed && (
        <span className="conversation-live-elapsed shrink-0 text-muted-foreground/70 tabular-nums">
          {elapsed}
        </span>
      )}
      {detail && (
        <span
          className="conversation-live-detail text-muted-foreground/75"
          title={detail}
        >
          {detail}
        </span>
      )}
    </span>
  )
}

export const LiveTurnStatusRow = memo(function LiveTurnStatusRow(
  props: LiveTurnStatusRowProps
) {
  const { pageVisible } = useConversationActivityVisibility()
  const state = useConversationPresentation()
  const scoped = state != null && state.contextKey === props.contextKey
  const compact = !props.isStreaming
  const childControl =
    scoped &&
    state.tasks.some(
      (task) =>
        !isTaskHistory(task) &&
        (task.kind === "subagent" || task.kind === "delegation")
    )
      ? null
      : props.subAgentControl
  return (
    <div
      className={
        compact
          ? "conversation-live-footer contents"
          : "conversation-live-footer conversation-live-status-bar mx-auto w-full max-w-[866px] shrink-0 px-4 py-1"
      }
      data-paused={!pageVisible}
      data-waiting={props.waiting}
      data-attention={props.attention}
    >
      {!compact && (
        <div className="conversation-live-status-content flex min-h-9 min-w-0 items-center gap-2">
          <FooterActivity {...props} scoped={scoped} />
          {!scoped && <StopTurnButton onCancel={props.onCancel} />}
        </div>
      )}
      {!scoped && !compact && (
        <LiveTurnPlan
          entries={props.planEntries}
          isStreaming={props.isStreaming}
        />
      )}
      {(childControl || props.trailingStatus) && (
        <div
          className={
            "flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground empty:hidden" +
            (compact ? " mx-auto w-full max-w-4xl px-4 py-1" : "")
          }
        >
          {childControl}
          {props.trailingStatus}
        </div>
      )}
    </div>
  )
})

function FooterActivity(props: LiveTurnStatusRowProps & { scoped: boolean }) {
  const t = useTranslations("ConversationPresentation")
  const { unreadProgress } = useConversationActivityVisibility()
  const { scoped } = props
  return (
    <>
      {scoped && props.onReturnToActivity ? (
        <button
          type="button"
          onClick={props.onReturnToActivity}
          title={t("backToActivity")}
          className="flex min-w-0 items-center gap-2 rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <TurnActivity {...props} />
          {unreadProgress && (
            <span className="conversation-unread-progress text-[11px] text-primary">
              {t("newProgress")}
            </span>
          )}
          <CornerUpLeft
            aria-hidden="true"
            className="size-3 shrink-0 text-muted-foreground/60"
          />
        </button>
      ) : (
        <TurnActivity {...props} />
      )}
    </>
  )
}
