"use client"

import { memo, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import {
  Brain,
  CircleDashed,
  FileText,
  Globe,
  Image,
  ListTodo,
  MessageCircle,
  RotateCcw,
  Search,
  Square,
  Terminal,
  Wrench,
} from "lucide-react"
import type { PlanEntryInfo } from "@/lib/types"
import { cn } from "@/lib/utils"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import type { ActivityIcon } from "./live-turn-activity"
import { LiveTurnElapsed } from "./live-turn-elapsed"
import { LiveTurnPlan } from "./live-turn-plan"

interface LiveTurnStatusRowProps {
  phase: string
  detail: string
  icon: ActivityIcon
  waiting: boolean
  attention: boolean
  startedAt: number | null
  isStreaming: boolean
  planEntries: PlanEntryInfo[]
  subAgentControl?: ReactNode
  trailingStatus?: ReactNode
  onCancel?: () => void
}

const ACTIVITY_ICONS = {
  image: Image,
  command: Terminal,
  search: Search,
  file: FileText,
  memory: Brain,
  browser: Globe,
  task: ListTodo,
  tool: Wrench,
  thinking: Brain,
  reply: MessageCircle,
  wait: CircleDashed,
  retry: RotateCcw,
  input: MessageCircle,
}

function ActivityGlyph({
  icon,
  waiting,
  attention,
}: Pick<LiveTurnStatusRowProps, "icon" | "waiting" | "attention">) {
  const Icon = ACTIVITY_ICONS[icon]
  return (
    <span
      aria-hidden="true"
      className={cn(
        "relative mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg @[28rem]/turnstats:size-9",
        waiting
          ? "bg-muted text-muted-foreground"
          : "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        attention && "bg-amber-500/10 text-amber-700 dark:text-amber-300"
      )}
    >
      <Icon className="size-[18px]" />
      {!waiting && (
        <span className="absolute -end-0.5 -bottom-0.5 size-2.5 animate-pulse rounded-full border-2 border-background bg-current motion-reduce:animate-none" />
      )}
    </span>
  )
}

const TurnActivity = memo(function TurnActivity({
  phase,
  detail,
  icon,
  waiting,
  attention,
}: Pick<
  LiveTurnStatusRowProps,
  "phase" | "detail" | "icon" | "waiting" | "attention"
>) {
  return (
    <div className="flex min-w-0 flex-1 items-start gap-2.5 @[28rem]/turnstats:gap-[13px]">
      <ActivityGlyph icon={icon} waiting={waiting} attention={attention} />
      <div
        className="min-w-0 pt-px"
        role="status"
        aria-live="polite"
        aria-atomic="true"
      >
        <div className="break-words text-sm leading-6 font-semibold text-foreground @[28rem]/turnstats:text-[15px]">
          {phase}
        </div>
        <p
          className="mt-0.5 line-clamp-2 min-h-5 break-words text-xs leading-5 text-muted-foreground"
          title={detail}
        >
          {detail}
        </p>
      </div>
    </div>
  )
})

function StopTurnButton({ onCancel }: { onCancel?: () => void }) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  if (!onCancel) return null
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={onCancel}
          aria-label={t("stop")}
          className="inline-flex size-8 shrink-0 items-center justify-center rounded-md bg-muted/60 text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-destructive/40"
        >
          <Square aria-hidden="true" className="size-3 fill-current" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="top">{t("stop")}</TooltipContent>
    </Tooltip>
  )
}

export const LiveTurnStatusRow = memo(function LiveTurnStatusRow(
  props: LiveTurnStatusRowProps
) {
  return (
    <div className="@container/turnstats shrink-0 px-4 pt-3 pb-2">
      <div className="flex min-h-14 items-start gap-2 @[28rem]/turnstats:gap-3">
        <TurnActivity
          phase={props.phase}
          detail={props.detail}
          icon={props.icon}
          waiting={props.waiting}
          attention={props.attention}
        />
        <TooltipProvider delayDuration={150}>
          <div className="flex shrink-0 items-center gap-2 pt-0.5 @[28rem]/turnstats:gap-3.5">
            <LiveTurnElapsed
              key={props.startedAt}
              startedAt={props.startedAt}
            />
            <StopTurnButton onCancel={props.onCancel} />
          </div>
        </TooltipProvider>
      </div>
      <LiveTurnPlan
        entries={props.planEntries}
        isStreaming={props.isStreaming}
      />
      {(props.subAgentControl || props.trailingStatus) && (
        <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground @[28rem]/turnstats:ms-[49px]">
          {props.subAgentControl}
          {props.trailingStatus}
        </div>
      )}
    </div>
  )
})
