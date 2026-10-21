"use client"

import { memo, useMemo } from "react"
import { MotionConfig } from "motion/react"
import { useTranslations } from "next-intl"
import { AgentIcon } from "@/components/agent-icon"
import { AssistantProcessSurface } from "./assistant-process-surface"
import {
  AssistantCurrentActivity,
  AssistantMonitorStatus,
} from "./assistant-current-activity"
import { ContentPartsRenderer } from "./content-parts-renderer"
import { countProcessItems, processPartHasError } from "./assistant-turn-model"
import { findImageRegistrationIssue } from "./assistant-turn-process"
import {
  buildAssistantTimeline,
  hasActiveProcess,
  type AssistantTimelineSection,
} from "./assistant-turn-timeline"
import { ImageArtifactRegistrationNotice } from "./assistant-turn-status"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import type { AgentType } from "@/lib/types"
import { useConversationActivityAnchor } from "./conversation-activity-visibility"
import { AssistantTextSurface } from "./assistant-text-surface"
import {
  processPartHasFailure,
  processPartNeedsAttention,
} from "./process-presentation"

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
  const sections = useMemo(
    () => buildAssistantTimeline(props.parts, props.isResponseComplete),
    [props.parts, props.isResponseComplete]
  )
  const issue = useMemo(
    () => findImageRegistrationIssue(props.parts),
    [props.parts]
  )
  return (
    <div className="conversation-assistant-turn space-y-3">
      <div className="conversation-assistant-header">
        <AssistantIdentity agentType={props.agentType} />
        {props.isLatest && !props.isResponseComplete && (
          <AssistantCurrentActivity
            conversationId={props.conversationId}
            animationEnabled={props.animationEnabled}
          />
        )}
      </div>
      <AssistantTimeline {...props} sections={sections} />
      {issue && <ImageArtifactRegistrationNotice state={issue} />}
      {props.isLatest && (
        <AssistantMonitorStatus
          parts={props.parts}
          conversationId={props.conversationId}
        />
      )}
    </div>
  )
})

function AssistantTimeline(
  props: AssistantTurnContentProps & { sections: AssistantTimelineSection[] }
) {
  return props.sections.map((section, index) => {
    const entranceKey = props.entranceKey + ":timeline:" + section.key
    if (section.kind === "process") {
      return (
        <AssistantProcessSurface
          key={section.key}
          {...props}
          entranceKey={entranceKey}
          parts={section.parts}
          isResponseComplete={
            props.isResponseComplete ||
            (index < props.sections.length - 1 &&
              !hasActiveProcess(section.parts) &&
              !section.parts.some(processPartNeedsAttention))
          }
          durationMs={null}
          processCount={countProcessItems(section.parts)}
          processHasError={
            section.parts.some(processPartHasError) ||
            section.parts.some(processPartHasFailure)
          }
        />
      )
    }
    const streaming =
      !props.isResponseComplete && index === props.sections.length - 1
    return (
      <div
        key={section.key}
        className="assistant-turn-summary"
        data-streaming={streaming && props.animationEnabled}
      >
        <MotionConfig
          reducedMotion={
            section.parts[0]?.type === "reasoning" ? "always" : "user"
          }
        >
          <AssistantSectionContent
            {...props}
            section={section}
            streaming={streaming}
            entranceKey={entranceKey}
          />
        </MotionConfig>
        {props.isLatest && index === props.sections.length - 1 && (
          <AssistantOutputAnchor complete={props.isResponseComplete} />
        )}
      </div>
    )
  })
}

function AssistantSectionContent(
  props: AssistantTurnContentProps & {
    section: AssistantTimelineSection
    streaming: boolean
  }
) {
  const content = (
    <ContentPartsRenderer
      parts={props.section.parts}
      role="assistant"
      entranceKey={props.entranceKey}
      animationEnabled={props.animationEnabled}
      conversationId={props.conversationId}
      intelligentUiStreaming={props.streaming}
    />
  )
  const textParts = props.section.parts.filter(
    (part): part is Extract<AdaptedContentPart, { type: "text" }> =>
      part.type === "text"
  )
  return textParts.length === props.section.parts.length ? (
    <AssistantTextSurface
      parts={textParts}
      streaming={props.streaming}
      compact={props.collapseCompletedTurn && props.displayMode !== "full"}
      animationEnabled={props.animationEnabled}
    >
      {content}
    </AssistantTextSurface>
  ) : (
    content
  )
}

function AssistantOutputAnchor({ complete }: { complete: boolean }) {
  const ref = useConversationActivityAnchor("output")
  return (
    <div
      ref={ref}
      className="conversation-output-anchor"
      data-streaming={!complete}
      aria-hidden="true"
    />
  )
}
