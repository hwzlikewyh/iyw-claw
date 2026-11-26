"use client"

import { useId, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { AssistantArchiveToggle } from "./assistant-archive-toggle"
import { AssistantFoldContent } from "./assistant-fold-content"
import type { AssistantTimelineSection } from "./assistant-turn-timeline"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import { buildAssistantArchiveEntries } from "./assistant-archive-segments"
import { AssistantProgressSegment } from "./assistant-progress-segment"
import {
  InterruptedTurnNotice,
  type TurnPresentation,
} from "./assistant-turn-completion"

const COMPLETION_LABELS = {
  completed: "turnCompleted",
  interrupted: "turnInterrupted",
  stopped: "turnStopped",
  unconfirmed: "turnUnconfirmed",
  ended: "turnEnded",
} as const

interface AssistantTurnArchiveProps {
  sections: AssistantTimelineSection[]
  complete: boolean
  outcome: TurnPresentation
  isLatest?: boolean
  collapse: boolean
  foldSegments?: boolean
  durationMs?: number | null
  renderSection: (
    section: AssistantTimelineSection,
    index: number,
    expanded: boolean
  ) => ReactNode
}

export function AssistantTurnArchive(props: AssistantTurnArchiveProps) {
  const t = useTranslations("Folder.chat.messageList")
  const live = useTranslations("Folder.chat.liveTurnStats")
  const contentId = useId()
  const hasHistory = props.sections.some(
    (section) => !section.retainOnCompletion
  )
  const canFold = props.complete && hasHistory
  const { rootRef, open, onOpenChange } = useArchiveDisclosure(
    canFold && props.collapse
  )
  const expanded = !canFold || open
  const labelKey = COMPLETION_LABELS[props.outcome]
  const label = completionLabel(props.durationMs, t(labelKey), live)
  const entries = buildAssistantArchiveEntries(props.sections, props.complete)
  const renderItem = (section: AssistantTimelineSection, index: number) => (
    <ArchiveSection
      key={section.key}
      section={section}
      index={index}
      expanded={expanded}
      renderSection={props.renderSection}
    />
  )
  return (
    <div ref={rootRef} className="assistant-turn-archive">
      {canFold ? (
        <AssistantArchiveToggle
          open={expanded}
          contentId={contentId}
          label={label}
          onToggle={() => onOpenChange(!expanded)}
        />
      ) : (
        props.complete && (
          <div className="assistant-turn-completion">{label}</div>
        )
      )}
      <div id={contentId}>
        {entries.map((entry) =>
          entry.kind === "section" ? (
            renderItem(entry.item.section, entry.item.index)
          ) : (
            <AssistantFoldContent key={entry.key} open={expanded}>
              <AssistantProgressSegment
                entry={entry}
                fold={props.foldSegments !== false}
              >
                {entry.items.map(({ section, index }) => (
                  <div key={section.key} className="assistant-timeline-section">
                    {props.renderSection(section, index, true)}
                  </div>
                ))}
              </AssistantProgressSegment>
            </AssistantFoldContent>
          )
        )}
      </div>
      {props.complete &&
        props.outcome !== "completed" &&
        props.outcome !== "ended" && (
          <InterruptedTurnNotice
            outcome={props.outcome}
            isLatest={Boolean(props.isLatest)}
          />
        )}
    </div>
  )
}

function completionLabel(
  duration: number | null | undefined,
  label: string,
  translate: Parameters<typeof formatElapsedLabel>[1]
) {
  return typeof duration === "number" && Number.isFinite(duration)
    ? label + " " + formatElapsedLabel(duration, translate)
    : label
}

function ArchiveSection({
  section,
  index,
  expanded,
  renderSection,
}: {
  section: AssistantTimelineSection
  index: number
  expanded: boolean
  renderSection: AssistantTurnArchiveProps["renderSection"]
}) {
  const content = (
    <div className="assistant-timeline-section">
      {renderSection(section, index, expanded)}
    </div>
  )
  // 最终正文和作品始终原样保留；过程的开关不控制结果。
  if (section.retainOnCompletion) return content
  return <AssistantFoldContent open={expanded}>{content}</AssistantFoldContent>
}
