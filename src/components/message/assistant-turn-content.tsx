"use client"

import { memo, useMemo, type ReactNode } from "react"
import { CheckCircle2, ChevronDown, ListChecks, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { AgentIcon } from "@/components/agent-icon"
import { ContentPartsRenderer } from "@/components/message/content-parts-renderer"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Badge } from "@/components/ui/badge"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import type { AgentType } from "@/lib/types"

interface AssistantTurnContentProps {
  agentType: AgentType
  parts: AdaptedContentPart[]
  entranceKey: string
  animationEnabled: boolean
  isResponseComplete: boolean
  displayMode: ConversationDisplayMode
  collapseCompletedTurn: boolean
  autoOpenErrors: boolean
  conversationId: number
  durationMs?: number | null
}

function hasError(part: AdaptedContentPart): boolean {
  if (part.type === "tool-call") {
    return part.state === "output-error" || Boolean(part.errorText?.trim())
  }
  if (part.type === "tool-result") {
    return part.state === "output-error" || Boolean(part.errorText?.trim())
  }
  if (part.type === "tool-group") return part.items.some(hasError)
  if (part.type === "delegation-status-group") return part.polls.some(hasError)
  if (part.type === "background-task-group") return part.polls.some(hasError)
  if (part.type === "goal-run") {
    return (
      hasError(part.start) ||
      Boolean(part.end && hasError(part.end)) ||
      part.items.some(hasError)
    )
  }
  return false
}

function countProcessItems(parts: AdaptedContentPart[]): number {
  return parts.reduce((count, part) => {
    if (part.type === "tool-group") return count + part.items.length
    if (part.type === "delegation-status-group")
      return count + part.polls.length
    if (part.type === "background-task-group") return count + part.polls.length
    if (part.type === "goal-run") {
      return count + 1 + countProcessItems(part.items) + (part.end ? 1 : 0)
    }
    if (part.type === "text" && part.text.trim().length === 0) return count
    return count + 1
  }, 0)
}

function findSummaryIndex(parts: AdaptedContentPart[]): number {
  for (let index = parts.length - 1; index >= 0; index -= 1) {
    const part = parts[index]
    if (part.type === "text" && part.text.trim().length > 0) return index
  }
  return -1
}

function isVisibleResultPart(part: AdaptedContentPart): boolean {
  return part.type === "generated-image" || part.type === "displayed-image"
}

function isReasoningPart(part: AdaptedContentPart): boolean {
  return part.type === "reasoning"
}

function AssistantIdentity({ agentType }: { agentType: AgentType }) {
  const t = useTranslations("Folder.chat.messageList")

  return (
    <div className="flex items-center gap-2 text-sm font-semibold">
      <AgentIcon
        agentType={agentType}
        className="size-6 rounded-full bg-muted p-0.5"
      />
      <span>{t("assistantName")}</span>
    </div>
  )
}

function CompletedProcessSummary({
  durationMs,
  processCount,
}: {
  durationMs?: number | null
  processCount: number
}) {
  const t = useTranslations("Folder.chat.messageList")
  const tLive = useTranslations("Folder.chat.liveTurnStats")
  const duration =
    typeof durationMs === "number" && durationMs > 0
      ? formatElapsedLabel(durationMs, tLive)
      : null
  const label = duration
    ? t("processCompleted", { duration })
    : t("processCompletedWithoutDuration")

  return (
    <span className="inline-flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <CheckCircle2 className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" />
      <span>{label}</span>
      {processCount > 0 && (
        <span className="text-muted-foreground/70">
          {t("processCount", { count: processCount })}
        </span>
      )}
    </span>
  )
}

function ProcessDisclosure({
  defaultOpen,
  entranceKey,
  processCount,
  processHasError,
  renderParts,
  processParts,
  durationMs,
}: {
  defaultOpen: boolean
  entranceKey: string
  processCount: number
  processHasError: boolean
  renderParts: (parts: AdaptedContentPart[], key: string) => ReactNode
  processParts: AdaptedContentPart[]
  durationMs?: number | null
}) {
  const t = useTranslations("Folder.chat.messageList")

  return (
    <Collapsible
      defaultOpen={defaultOpen}
      className="overflow-hidden rounded-md border border-border/70 bg-muted/20"
    >
      <CollapsibleTrigger className="group flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <ChevronDown className="size-3.5 shrink-0 text-muted-foreground transition-transform group-data-[state=closed]:-rotate-90" />
        <ListChecks className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1">
          <CompletedProcessSummary
            durationMs={durationMs}
            processCount={processCount}
          />
        </span>
        {processHasError && (
          <Badge variant="destructive" className="h-5 shrink-0 text-[10px]">
            {t("processHasErrors")}
          </Badge>
        )}
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t px-3 py-3 data-[state=closed]:animate-out data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0">
        {renderParts(processParts, `${entranceKey}:process`)}
      </CollapsibleContent>
    </Collapsible>
  )
}

export const AssistantTurnContent = memo(function AssistantTurnContent({
  agentType,
  parts,
  entranceKey,
  animationEnabled,
  isResponseComplete,
  displayMode,
  collapseCompletedTurn,
  autoOpenErrors,
  conversationId,
  durationMs,
}: AssistantTurnContentProps) {
  const t = useTranslations("Folder.chat.messageList")
  const summaryIndex = useMemo(
    () => (isResponseComplete ? findSummaryIndex(parts) : -1),
    [isResponseComplete, parts]
  )
  const summaryParts = summaryIndex >= 0 ? [parts[summaryIndex]] : []
  const resultParts = parts.filter(isVisibleResultPart)
  const reasoningParts = parts.filter(isReasoningPart)
  const processParts = parts.filter(
    (_, index) =>
      index !== summaryIndex &&
      !isVisibleResultPart(parts[index]) &&
      !isReasoningPart(parts[index])
  )
  const processCount = useMemo(
    () => countProcessItems(processParts),
    [processParts]
  )
  const processHasError = useMemo(
    () => processParts.some(hasError),
    [processParts]
  )

  const renderParts = (nextParts: AdaptedContentPart[], key: string) => (
    <ContentPartsRenderer
      parts={nextParts}
      role="assistant"
      entranceKey={key}
      animationEnabled={animationEnabled}
      conversationId={conversationId}
    />
  )

  if (!isResponseComplete) {
    if (displayMode !== "minimal") {
      return (
        <div className="space-y-3">
          <AssistantIdentity agentType={agentType} />
          {reasoningParts.length > 0 &&
            renderParts(reasoningParts, `${entranceKey}:reasoning`)}
          {processParts.length > 0 &&
            renderParts(processParts, `${entranceKey}:process`)}
        </div>
      )
    }
    const visibleParts = parts.filter(
      (part) => part.type === "text" || isVisibleResultPart(part)
    )
    return (
      <div className="space-y-2">
        <AssistantIdentity agentType={agentType} />
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Loader2 className="size-3 shrink-0 animate-spin motion-reduce:animate-none" />
          {t("processRunning")}
        </div>
        {visibleParts.length > 0
          ? renderParts(visibleParts, `${entranceKey}:minimal`)
          : null}
      </div>
    )
  }

  const defaultOpen =
    !collapseCompletedTurn ||
    displayMode === "full" ||
    (autoOpenErrors && processHasError)
  const collapseKey = `${entranceKey}:completed:${displayMode}:${collapseCompletedTurn}:${autoOpenErrors}:${processHasError}`
  const hasVisibleSummary = summaryParts.length > 0 || resultParts.length > 0
  const showProcess =
    displayMode !== "minimal" || processHasError || !hasVisibleSummary

  return (
    <div className="space-y-3">
      <AssistantIdentity agentType={agentType} />
      {processCount > 0 && showProcess ? (
        <ProcessDisclosure
          key={collapseKey}
          defaultOpen={defaultOpen}
          entranceKey={entranceKey}
          processCount={processCount}
          processHasError={processHasError}
          processParts={processParts}
          renderParts={renderParts}
          durationMs={durationMs}
        />
      ) : (
        <CompletedProcessSummary
          durationMs={durationMs}
          processCount={processCount}
        />
      )}

      {reasoningParts.length > 0 &&
        renderParts(reasoningParts, `${entranceKey}:reasoning`)}

      {summaryParts.length > 0 ? (
        <div className="assistant-turn-summary">
          {renderParts(summaryParts, `${entranceKey}:summary`)}
        </div>
      ) : null}
      {resultParts.length > 0 ? (
        <div className="assistant-turn-results">
          {renderParts(resultParts, `${entranceKey}:results`)}
        </div>
      ) : null}
    </div>
  )
})
