"use client"

import { memo, useMemo } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { AgentIcon } from "@/components/agent-icon"
import { AssistantProcessSurface } from "@/components/message/assistant-process-surface"
import { ContentPartsRenderer } from "@/components/message/content-parts-renderer"
import {
  countProcessItems,
  processPartHasError,
  splitAssistantTurnParts,
} from "@/components/message/assistant-turn-model"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
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
  const sections = useMemo(
    () => splitAssistantTurnParts(parts, isResponseComplete),
    [isResponseComplete, parts]
  )
  const processCount = useMemo(
    () => countProcessItems(sections.processParts),
    [sections.processParts]
  )
  const processHasError = useMemo(
    () => sections.processParts.some(processPartHasError),
    [sections.processParts]
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
  const processSurface = (
    <AssistantProcessSurface
      parts={sections.processParts}
      processCount={processCount}
      processHasError={processHasError}
      entranceKey={entranceKey}
      animationEnabled={animationEnabled}
      isResponseComplete={isResponseComplete}
      displayMode={displayMode}
      collapseCompletedTurn={collapseCompletedTurn}
      autoOpenErrors={autoOpenErrors}
      conversationId={conversationId}
      durationMs={durationMs}
    />
  )

  if (!isResponseComplete) {
    return (
      <div className="space-y-3">
        <AssistantIdentity agentType={agentType} />
        {displayMode === "minimal" ? (
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 className="size-3 shrink-0 animate-spin motion-reduce:animate-none" />
            {t("processRunning")}
          </div>
        ) : (
          processSurface
        )}
        {sections.resultParts.length > 0 &&
          renderParts(sections.resultParts, `${entranceKey}:results`)}
      </div>
    )
  }

  const hasVisibleSummary =
    sections.summaryParts.length > 0 || sections.resultParts.length > 0
  const showProcess =
    processCount > 0 &&
    (displayMode !== "minimal" || processHasError || !hasVisibleSummary)

  return (
    <div className="space-y-3">
      <AssistantIdentity agentType={agentType} />
      {showProcess ? processSurface : null}
      {sections.summaryParts.length > 0 ? (
        <div className="assistant-turn-summary">
          {renderParts(sections.summaryParts, `${entranceKey}:summary`)}
        </div>
      ) : null}
      {sections.resultParts.length > 0 ? (
        <div className="assistant-turn-results">
          {renderParts(sections.resultParts, `${entranceKey}:results`)}
        </div>
      ) : null}
    </div>
  )
})
