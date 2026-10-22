"use client"

import { useId, type ReactNode } from "react"
import { AssistantArchiveToggle } from "./assistant-archive-toggle"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useTranslations } from "next-intl"
import type { AssistantTimelineSection } from "./assistant-turn-timeline"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import { AssistantFinalResponsePreview } from "./assistant-final-response-preview"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import {
  buildAssistantArchiveEntries,
  type AssistantArchiveEntry,
} from "./assistant-archive-segments"

interface AssistantTurnArchiveProps {
  sections: AssistantTimelineSection[]
  complete: boolean
  collapse: boolean
  durationMs?: number | null
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
  const hasFinalResponse = props.sections.some(
    (section) => section.isFinalResponse
  )
  const canFold = props.complete && (hasHistory || hasFinalResponse)
  const entries = buildAssistantArchiveEntries(props.sections)
  const { rootRef, open, onOpenChange, hasChoice } = useArchiveDisclosure(
    canFold && props.collapse
  )
  const live = useTranslations("Folder.chat.liveTurnStats")
  const expanded = !canFold || open
  return (
    <div ref={rootRef} className="assistant-turn-archive">
      {canFold && (
        <AssistantArchiveToggle
          open={expanded}
          contentId={contentId}
          label={completionLabel(props.durationMs, t, live)}
          onToggle={() => onOpenChange(!expanded)}
        />
      )}
      <ArchiveSections
        contentId={contentId}
        entries={entries}
        expanded={expanded}
        compactFinal={canFold && !expanded}
        userExpanded={canFold && expanded && hasChoice}
        renderSection={props.renderSection}
      />
      {props.complete &&
        !props.sections.some((section) => section.retainOnCompletion) && (
          <p className="text-sm text-muted-foreground">
            {t("conversationSummaryFallback")}
          </p>
        )}
    </div>
  )
}

function ArchiveSections({
  contentId,
  entries,
  expanded,
  compactFinal,
  userExpanded,
  renderSection,
}: {
  contentId: string
  entries: AssistantArchiveEntry[]
  expanded: boolean
  compactFinal: boolean
  userExpanded: boolean
  renderSection: AssistantTurnArchiveProps["renderSection"]
}) {
  return (
    <div id={contentId}>
      {entries.map((entry) =>
        entry.kind === "group" ? (
          <AssistantFoldContent key={entry.key} open={!compactFinal}>
            <ArchivedSectionGroup
              entry={entry}
              forceOpen={userExpanded}
              renderSection={renderSection}
            />
          </AssistantFoldContent>
        ) : (
          <ArchivedSection
            key={entry.key}
            section={entry.item.section}
            open={
              expanded ||
              (entry.item.section.retainOnCompletion &&
                !entry.item.section.isFinalResponse)
            }
            compactFinal={compactFinal && entry.item.section.isFinalResponse}
            expanded={userExpanded}
            index={entry.item.index}
            renderSection={renderSection}
          />
        )
      )}
    </div>
  )
}

function ArchivedSectionGroup({
  entry,
  forceOpen,
  renderSection,
}: {
  entry: Extract<AssistantArchiveEntry, { kind: "group" }>
  forceOpen: boolean
  renderSection: AssistantTurnArchiveProps["renderSection"]
}) {
  const t = useTranslations("Folder.chat.messageList")
  const contentId = useId()
  const { rootRef, open, onOpenChange } = useArchiveDisclosure(true)
  const visible = forceOpen || open
  const key = visible ? "collapseArchivedSegments" : "expandArchivedSegments"
  return (
    <div ref={rootRef} className="assistant-process-surface">
      <AssistantArchiveToggle
        open={visible}
        contentId={contentId}
        label={t(key, { count: entry.items.length })}
        onToggle={() => onOpenChange(!visible)}
      />
      <AssistantFoldContent open={visible} contentId={contentId}>
        {entry.items.map(({ section, index }) => (
          <ArchivedSection
            key={section.key}
            section={section}
            open
            compactFinal={false}
            expanded
            index={index}
            renderSection={renderSection}
          />
        ))}
      </AssistantFoldContent>
    </div>
  )
}

type ArchiveTranslate = (key: "turnCompleted") => string

function completionLabel(
  durationMs: number | null | undefined,
  translate: ArchiveTranslate,
  live: Parameters<typeof formatElapsedLabel>[1]
) {
  if (typeof durationMs !== "number" || !Number.isFinite(durationMs))
    return translate("turnCompleted")
  return translate("turnCompleted") + " " + formatElapsedLabel(durationMs, live)
}

type AdaptedTimelinePart = AssistantTimelineSection["parts"][number]

function ArchivedSection({
  section,
  open,
  compactFinal,
  expanded,
  index,
  renderSection,
}: {
  section: AssistantTimelineSection
  open: boolean
  compactFinal: boolean
  expanded: boolean
  index: number
  renderSection: AssistantTurnArchiveProps["renderSection"]
}) {
  const parts = section.parts.filter(
    (part): part is Extract<AdaptedTimelinePart, { type: "text" }> =>
      part.type === "text"
  )
  if (section.isFinalResponse) {
    return (
      <>
        {compactFinal && <AssistantFinalResponsePreview parts={parts} />}
        <AssistantFoldContent open={open}>
          {open && (
            <div className="pb-3">
              {renderSection(section, index, expanded)}
            </div>
          )}
        </AssistantFoldContent>
      </>
    )
  }
  return (
    <AssistantFoldContent open={open}>
      {open && (
        <div className="pb-3">{renderSection(section, index, expanded)}</div>
      )}
    </AssistantFoldContent>
  )
}
