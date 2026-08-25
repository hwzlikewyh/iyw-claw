"use client"

import type { ReactNode } from "react"
import { useTranslations } from "next-intl"
import {
  CircleDashed,
  ListTodoIcon,
  Loader2,
  Square,
  Timer,
  Wrench,
} from "lucide-react"
import type {
  LiveMessage,
  ToolCallInfo,
} from "@/contexts/acp-connections-context"
import type { PlanEntryInfo } from "@/lib/types"
import { Badge } from "@/components/ui/badge"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { PlanEntriesList } from "@/components/message/plan-card"

interface LiveTurnStatusRowProps {
  message: LiveMessage | null
  modelName?: string | null
  isStreaming: boolean
  planEntries: PlanEntryInfo[]
  completedPlanCount: number
  elapsedLabel: string | null
  toolCallCount: number
  isAwaitingUserInput?: boolean
  subAgentControl?: ReactNode
  trailingStatus?: ReactNode
  onCancel?: () => void
}

function latestToolWithStatus(
  message: LiveMessage | null,
  status: string
): ToolCallInfo | null {
  if (!message) return null
  for (let index = message.content.length - 1; index >= 0; index -= 1) {
    const block = message.content[index]
    if (block.type === "tool_call" && block.info.status === status) {
      return block.info
    }
  }
  return null
}

function latestActiveTool(message: LiveMessage | null): ToolCallInfo | null {
  return (
    latestToolWithStatus(message, "in_progress") ??
    latestToolWithStatus(message, "pending")
  )
}

function currentPlanStep(entries: PlanEntryInfo[]): string | null {
  return (
    entries.find((entry) => entry.status === "in_progress")?.content.trim() ||
    null
  )
}

function phaseLabel(
  message: LiveMessage | null,
  planEntries: PlanEntryInfo[],
  workingLabel: string
): string {
  const toolTitle = latestActiveTool(message)?.title.trim()
  if (toolTitle) return toolTitle

  const planStep = currentPlanStep(planEntries)
  if (planStep) return planStep

  return workingLabel
}

function PlanProgress({
  entries,
  completedCount,
  isStreaming,
}: {
  entries: PlanEntryInfo[]
  completedCount: number
  isStreaming: boolean
}) {
  const t = useTranslations("Folder.chat.agentPlanOverlay")
  if (entries.length === 0) return null

  return (
    <>
      <Popover>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="inline-flex h-5 items-center gap-1 rounded-full px-1.5 leading-none text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <ListTodoIcon className="size-3 shrink-0" />
            <span>{t("title")}</span>
            <Badge variant="secondary" className="h-4 px-1 text-[10px]">
              {completedCount}/{entries.length}
            </Badge>
          </button>
        </PopoverTrigger>
        <PlanProgressContent
          entries={entries}
          completedCount={completedCount}
          isStreaming={isStreaming}
        />
      </Popover>
      <Separator />
    </>
  )
}

function PlanProgressContent({
  entries,
  completedCount,
  isStreaming,
}: {
  entries: PlanEntryInfo[]
  completedCount: number
  isStreaming: boolean
}) {
  const t = useTranslations("Folder.chat.agentPlanOverlay")
  return (
    <PopoverContent
      side="top"
      align="center"
      className="w-80 max-w-[calc(100vw-2rem)] gap-0 overflow-hidden p-0"
    >
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <ListTodoIcon className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {t("title")}
        </span>
        <Badge variant="secondary" className="h-5 shrink-0">
          {completedCount}/{entries.length}
        </Badge>
      </div>
      <div className="max-h-72 overflow-y-auto p-2">
        <PlanEntriesList entries={entries} isStreaming={isStreaming} />
      </div>
    </PopoverContent>
  )
}

function Separator({ responsive = false }: { responsive?: boolean }) {
  return (
    <span
      className={
        responsive
          ? "hidden text-border @[30rem]/turnstats:inline"
          : "text-border"
      }
    >
      |
    </span>
  )
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
            className="inline-flex size-6 items-center justify-center rounded-full text-destructive transition-colors hover:bg-destructive/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-destructive/40"
          >
            <Square className="size-3 fill-current" />
          </button>
        </TooltipTrigger>
        <TooltipContent side="top">{t("stop")}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

function TurnFacts(props: LiveTurnStatusRowProps) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  return (
    <>
      {props.elapsedLabel && (
        <>
          <Separator />
          <span className="inline-flex items-center gap-1">
            <Timer className="size-3 shrink-0" />
            {props.elapsedLabel}
          </span>
        </>
      )}
      {props.toolCallCount > 0 && (
        <>
          <Separator responsive />
          <span className="hidden items-center gap-1 @[30rem]/turnstats:inline-flex">
            <Wrench className="size-3 shrink-0" />
            {t("toolUseCount", { count: props.toolCallCount })}
          </span>
        </>
      )}
      {props.trailingStatus}
      <StopTurnButton onCancel={props.onCancel} />
    </>
  )
}

export function LiveTurnStatusRow(props: LiveTurnStatusRowProps) {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const phase = props.isAwaitingUserInput
    ? t("awaitingUser")
    : phaseLabel(props.message, props.planEntries, t("working"))

  return (
    <div className="@container/turnstats shrink-0">
      <div className="flex min-h-8 flex-wrap items-center justify-center gap-x-3 gap-y-1 px-4 py-1 text-xs leading-none text-muted-foreground">
        <PlanProgress
          entries={props.planEntries}
          completedCount={props.completedPlanCount}
          isStreaming={props.isStreaming}
        />
        {props.subAgentControl && (
          <>
            {props.subAgentControl}
            <Separator />
          </>
        )}
        <span className="inline-flex min-w-0 max-w-[min(24rem,55vw)] items-center gap-1.5">
          {props.isAwaitingUserInput ? (
            <CircleDashed className="size-3 shrink-0" />
          ) : (
            <Loader2 className="size-3 shrink-0 animate-spin motion-reduce:animate-none" />
          )}
          <span className="truncate">{phase}</span>
        </span>
        <TurnFacts {...props} />
      </div>
    </div>
  )
}
