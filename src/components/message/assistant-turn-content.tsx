"use client"

import { memo, useMemo } from "react"
import { ChevronDown, ListChecks } from "lucide-react"
import { useTranslations } from "next-intl"

import { ContentPartsRenderer } from "@/components/message/content-parts-renderer"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Badge } from "@/components/ui/badge"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { cn } from "@/lib/utils"

interface AssistantTurnContentProps {
  parts: AdaptedContentPart[]
  entranceKey: string
  animationEnabled: boolean
  isResponseComplete: boolean
  displayMode: ConversationDisplayMode
  collapseCompletedTurn: boolean
  autoOpenErrors: boolean
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

export const AssistantTurnContent = memo(function AssistantTurnContent({
  parts,
  entranceKey,
  animationEnabled,
  isResponseComplete,
  displayMode,
  collapseCompletedTurn,
  autoOpenErrors,
}: AssistantTurnContentProps) {
  const t = useTranslations("Folder.chat.messageList")
  const summaryIndex = useMemo(
    () => (isResponseComplete ? findSummaryIndex(parts) : -1),
    [isResponseComplete, parts]
  )
  const summaryParts = summaryIndex >= 0 ? [parts[summaryIndex]] : []
  const resultParts = parts.filter(isVisibleResultPart)
  const processParts = parts.filter(
    (_, index) => index !== summaryIndex && !isVisibleResultPart(parts[index])
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
    />
  )

  if (!isResponseComplete) {
    if (displayMode !== "minimal") return renderParts(parts, entranceKey)
    const visibleParts = parts.filter(
      (part) => part.type === "text" || isVisibleResultPart(part)
    )
    return (
      <div className="space-y-2">
        <div className="text-xs text-muted-foreground">
          {t("processRunning")}
        </div>
        {visibleParts.length > 0
          ? renderParts(visibleParts, `${entranceKey}:minimal`)
          : null}
      </div>
    )
  }

  if (processCount === 0) return renderParts(parts, entranceKey)

  const defaultOpen =
    !collapseCompletedTurn ||
    displayMode === "full" ||
    (autoOpenErrors && processHasError)
  const collapseKey = `${entranceKey}:completed:${displayMode}:${collapseCompletedTurn}:${autoOpenErrors}:${processHasError}`
  const hasVisibleSummary = summaryParts.length > 0 || resultParts.length > 0
  const showProcess =
    displayMode !== "minimal" || processHasError || !hasVisibleSummary

  if (!showProcess) {
    return (
      <div className="space-y-3">
        {summaryParts.length > 0
          ? renderParts(summaryParts, `${entranceKey}:summary`)
          : null}
        {resultParts.length > 0
          ? renderParts(resultParts, `${entranceKey}:results`)
          : null}
      </div>
    )
  }

  return (
    <div className="space-y-3">
      <Collapsible
        key={collapseKey}
        defaultOpen={defaultOpen}
        className={cn(
          "overflow-hidden rounded-lg border bg-muted/20",
          processHasError && "border-destructive/40 bg-destructive/5"
        )}
      >
        <CollapsibleTrigger
          className={cn(
            "group flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left text-xs transition-colors hover:bg-muted/40",
            processHasError && "hover:bg-destructive/10"
          )}
        >
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground transition-transform group-data-[state=closed]:-rotate-90" />
          <ListChecks className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate font-medium">
            {processHasError
              ? t("processWithErrors", { count: processCount })
              : t("processSummary", { count: processCount })}
          </span>
          <Badge
            variant={processHasError ? "destructive" : "outline"}
            className="shrink-0 text-[10px]"
          >
            {t("processDetails")}
          </Badge>
        </CollapsibleTrigger>
        <CollapsibleContent className="border-t px-3 py-3 data-[state=closed]:animate-out data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0">
          {renderParts(processParts, `${entranceKey}:process`)}
        </CollapsibleContent>
      </Collapsible>

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
