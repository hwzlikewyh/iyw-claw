"use client"

import { memo, useMemo } from "react"
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
  const responseContent =
    sections.responseParts.length > 0
      ? renderParts(sections.responseParts, `${entranceKey}:response`)
      : null

  if (!isResponseComplete) {
    return (
      <div className="space-y-3">
        <AssistantIdentity agentType={agentType} />
        {processCount > 0 ? processSurface : null}
        {sections.resultParts.length > 0 &&
          renderParts(sections.resultParts, `${entranceKey}:results`)}
      </div>
    )
  }

  const showProcess = processCount > 0

  return (
    <div className="space-y-3">
      <AssistantIdentity agentType={agentType} />
      {showProcess ? processSurface : null}
      {responseContent ? (
        <div className="assistant-turn-summary">{responseContent}</div>
      ) : null}
      {sections.resultParts.length > 0 ? (
        <div className="assistant-turn-results">
          {renderParts(sections.resultParts, `${entranceKey}:results`)}
        </div>
      ) : null}
    </div>
  )
})
