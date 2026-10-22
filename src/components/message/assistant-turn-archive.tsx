"use client"

import { useId, type ReactNode } from "react"
import { AssistantArchiveToggle } from "./assistant-archive-toggle"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useTranslations } from "next-intl"
import type { AssistantTimelineSection } from "./assistant-turn-timeline"
import { useArchiveDisclosure } from "./use-archive-disclosure"

interface AssistantTurnArchiveProps {
  sections: AssistantTimelineSection[]
  complete: boolean
  collapse: boolean
  renderSection: (
    section: AssistantTimelineSection,
    index: number,
    expanded: boolean
  ) => ReactNode
}

export function AssistantTurnArchive(props: AssistantTurnArchiveProps) {
  const t = useTranslations("Folder.chat.messageList")
  const contentId = useId()
  const hasHistory = props.sections.some(
    (section) => !section.retainOnCompletion
  )
  const canFold = props.complete && hasHistory
  const { rootRef, open, onOpenChange, hasChoice } = useArchiveDisclosure(
    canFold && props.collapse
  )
  const expanded = !canFold || open
  return (
    <div ref={rootRef} className="assistant-turn-archive">
      {canFold && (
        <AssistantArchiveToggle
          open={expanded}
          contentId={contentId}
          label={t(expanded ? "collapseConversation" : "expandConversation")}
          onToggle={() => onOpenChange(!expanded)}
        />
      )}
      <div id={contentId}>
        {props.sections.map((section, index) => (
          <AssistantFoldContent
            key={section.key}
            open={expanded || section.retainOnCompletion}
          >
            <div className="pb-3">
              {props.renderSection(
                section,
                index,
                canFold && expanded && hasChoice
              )}
            </div>
          </AssistantFoldContent>
        ))}
      </div>
      {props.complete &&
        !props.sections.some((section) => section.retainOnCompletion) && (
          <p className="text-sm text-muted-foreground">
            {t("conversationSummaryFallback")}
          </p>
        )}
    </div>
  )
}
