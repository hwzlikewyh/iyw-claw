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
import { AssistantContentSegment } from "./assistant-content-segment"
import { AssistantTurnArchive } from "./assistant-turn-archive"

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
            parts={props.parts}
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
  return (
    <AssistantTurnArchive
      sections={props.sections}
      complete={props.isResponseComplete}
      collapse={props.collapseCompletedTurn}
      durationMs={props.durationMs}
      renderSection={(section, index, expanded) => (
        <AssistantTimelineSectionView
          {...props}
          section={section}
          latest={index === props.sections.length - 1}
          expanded={expanded}
        />
      )}
    />
  )
}

function AssistantTimelineSectionView(
  props: AssistantTurnContentProps & {
    section: AssistantTimelineSection
    latest: boolean
    expanded: boolean
  }
) {
  const { section } = props
  const entranceKey = props.entranceKey + ":timeline:" + section.key
  if (section.kind === "process")
    return <AssistantProcessSection {...props} entranceKey={entranceKey} />
  const streaming = !props.isResponseComplete && props.latest
  const archive =
    section.kind === "content" &&
    section.parts.every((part) => part.type === "text") &&
    !streaming &&
    !props.expanded &&
    !(props.isResponseComplete && section.retainOnCompletion) &&
    props.collapseCompletedTurn &&
    props.displayMode !== "full"
  return (
    <AssistantContentSegment archive={archive}>
      <div
        className="assistant-turn-summary"
        data-streaming={streaming && props.animationEnabled}
      >
        <AssistantSectionContent
          {...props}
          section={section}
          streaming={streaming}
          entranceKey={entranceKey}
        />
        {props.isLatest && props.latest && (
          <AssistantOutputAnchor complete={props.isResponseComplete} />
        )}
      </div>
    </AssistantContentSegment>
  )
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
      compact={false}
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

function AssistantProcessSection(
  props: AssistantTurnContentProps & {
    section: AssistantTimelineSection
    latest: boolean
    expanded: boolean
  }
) {
  return (
    <AssistantProcessSurface
      {...props}
      parts={props.section.parts}
      isResponseComplete={
        props.isResponseComplete ||
        (!props.latest && !hasActiveProcess(props.section.parts))
      }
      collapseCompletedTurn={!props.expanded && props.collapseCompletedTurn}
      durationMs={null}
    />
  )
}
