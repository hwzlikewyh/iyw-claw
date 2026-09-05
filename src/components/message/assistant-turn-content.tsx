"use client"

import { memo, useMemo, type ReactNode } from "react"
import { ChevronDown, ListChecks, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  AssistantIdentity,
  CompletedProcessSummary,
  ImageArtifactRegistrationNotice,
} from "@/components/message/assistant-turn-status"
import {
  completedProcessPart,
  countProcessItems,
  findImageRegistrationIssue,
  findSummaryIndex,
  hasProcessError,
  isFinalResultPart,
  isLiveVisibleResultPart,
  isReasoningPart,
} from "@/components/message/assistant-turn-process"
import { ContentPartsRenderer } from "@/components/message/content-parts-renderer"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Badge } from "@/components/ui/badge"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
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
  const resultParts = parts.filter(isFinalResultPart)
  const reasoningParts = parts.filter(isReasoningPart)
  const processParts = parts.flatMap((part, index) => {
    if (
      index === summaryIndex ||
      isFinalResultPart(part) ||
      isReasoningPart(part)
    ) {
      return []
    }
    if (!isResponseComplete) return [part]
    const visible = completedProcessPart(part)
    return visible ? [visible] : []
  })
  const processCount = useMemo(
    () => countProcessItems(processParts),
    [processParts]
  )
  const processHasError = useMemo(
    () =>
      processParts.some(hasProcessError) ||
      (isResponseComplete && parts.some(hasProcessError)),
    [isResponseComplete, parts, processParts]
  )
  const registrationIssue = useMemo(
    () => findImageRegistrationIssue(parts),
    [parts]
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
      (part) =>
        part.type === "text" ||
        isReasoningPart(part) ||
        isLiveVisibleResultPart(part)
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
          hasError={processHasError}
        />
      )}
      {registrationIssue && (
        <ImageArtifactRegistrationNotice state={registrationIssue} />
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
