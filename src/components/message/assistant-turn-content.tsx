"use client"

import { memo, useMemo } from "react"
import { useTranslations } from "next-intl"
import { AgentIcon } from "@/components/agent-icon"
import { AssistantProcessSurface } from "./assistant-process-surface"
import {
  AssistantCurrentActivity,
  AssistantMonitorStatus,
} from "./assistant-current-activity"
import { ContentPartsRenderer } from "./content-parts-renderer"
import {
  countProcessItems,
  processPartHasError,
  splitAssistantTurnParts,
} from "./assistant-turn-model"
import {
  findImageRegistrationIssue,
  hasProcessError,
  visibleProcessPart,
} from "./assistant-turn-process"
import { ImageArtifactRegistrationNotice } from "./assistant-turn-status"
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
  isLatest?: boolean
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

export const AssistantTurnContent = memo(function AssistantTurnContent(
  props: AssistantTurnContentProps
) {
  const {
    sections,
    processParts,
    processCount,
    processHasError,
    registrationIssue,
    showProcess,
  } = useAssistantTurnPresentation(props)
  return (
    <div className="conversation-assistant-turn space-y-3">
      <AssistantIdentity agentType={props.agentType} />
      {props.isLatest && !props.isResponseComplete && (
        <AssistantCurrentActivity conversationId={props.conversationId} />
      )}
      {showProcess && (
        <AssistantProcessSurface
          {...props}
          parts={processParts}
          processCount={processCount}
          processHasError={processHasError}
        />
      )}
      {registrationIssue && (
        <ImageArtifactRegistrationNotice state={registrationIssue} />
      )}
      <AssistantBody {...props} sections={sections} />
      {props.isLatest && (
        <AssistantMonitorStatus
          parts={props.parts}
          conversationId={props.conversationId}
        />
      )}
    </div>
  )
})

function useAssistantTurnPresentation(props: AssistantTurnContentProps) {
  const sections = useMemo(
    () => splitAssistantTurnParts(props.parts, props.isResponseComplete),
    [props.parts, props.isResponseComplete]
  )
  const processParts = useMemo(
    () =>
      sections.processParts.flatMap((part) => {
        const visible = visibleProcessPart(part)
        return visible ? [visible] : []
      }),
    [sections.processParts]
  )
  const processCount = useMemo(
    () => countProcessItems(processParts),
    [processParts]
  )
  const processHasError = useMemo(
    () =>
      processParts.some(processPartHasError) ||
      (props.isResponseComplete && props.parts.some(hasProcessError)),
    [processParts, props.parts, props.isResponseComplete]
  )
  const registrationIssue = useMemo(
    () => findImageRegistrationIssue(props.parts),
    [props.parts]
  )
  const showProcess =
    processCount > 0 &&
    (props.displayMode !== "minimal" ||
      processHasError ||
      (sections.responseParts.length === 0 &&
        sections.resultParts.length === 0))
  return {
    sections,
    processParts,
    processCount,
    processHasError,
    registrationIssue,
    showProcess,
  }
}

function AssistantBody(
  props: AssistantTurnContentProps & {
    sections: ReturnType<typeof splitAssistantTurnParts>
  }
) {
  const { sections } = props
  return (
    <>
      <div
        className="assistant-turn-summary"
        data-streaming={!props.isResponseComplete && props.animationEnabled}
      >
        {sections.responseParts.length > 0 && (
          <ContentPartsRenderer
            parts={sections.responseParts}
            role="assistant"
            entranceKey={`${props.entranceKey}:response`}
            animationEnabled={props.animationEnabled}
            conversationId={props.conversationId}
            intelligentUiStreaming={!props.isResponseComplete}
          />
        )}
      </div>
      {sections.resultParts.length > 0 && (
        <div className="assistant-turn-results">
          <ContentPartsRenderer
            parts={sections.resultParts}
            role="assistant"
            entranceKey={`${props.entranceKey}:results`}
            animationEnabled={props.animationEnabled}
            conversationId={props.conversationId}
            intelligentUiStreaming={!props.isResponseComplete}
          />
        </div>
      )}
    </>
  )
}
