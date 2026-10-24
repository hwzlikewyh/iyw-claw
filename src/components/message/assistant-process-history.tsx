"use client"

import { useId, type ReactNode } from "react"
import { ChevronRight, History } from "lucide-react"
import { useTranslations } from "next-intl"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import { useArchiveSummary } from "./use-archive-summary"
import {
  processPartHasFailure,
  processPartNeedsAttention,
} from "./process-presentation"

const HISTORY_THRESHOLD = 8
const VISIBLE_RECENT_ACTIONS = 4
const HISTORY_SEGMENT_SIZE = 4

export function ProcessArchiveStatus({
  parts,
}: {
  parts: AdaptedContentPart[]
}) {
  const t = useTranslations("Folder.chat.contentParts.processStep")
  const running = parts.filter(
    (part) => !processPartHasFailure(part) && processPartNeedsAttention(part)
  ).length
  return (
    <>
      {running > 0 && (
        <span className="assistant-history-status is-running">
          {t("running")} · {running}
        </span>
      )}
    </>
  )
}

export function AssistantProcessHistory({
  parts,
  enabled,
  renderPart,
}: {
  parts: AdaptedContentPart[]
  enabled: boolean
  renderPart: (part: AdaptedContentPart, index: number) => ReactNode
}) {
  const archive = enabled && parts.length >= HISTORY_THRESHOLD
  // 固定段边界，后续输出不会把用户已展开的步骤移到另一个折叠组件。
  const cutoff = archive
    ? Math.floor(
        (parts.length - VISIBLE_RECENT_ACTIONS) / HISTORY_SEGMENT_SIZE
      ) * HISTORY_SEGMENT_SIZE
    : 0
  const segments = []
  for (let start = 0; start < cutoff; start += HISTORY_SEGMENT_SIZE) {
    segments.push(
      <ProcessHistorySegment
        key={start}
        start={start}
        parts={parts.slice(start, start + HISTORY_SEGMENT_SIZE)}
        renderPart={renderPart}
      />
    )
  }
  return (
    <div className="assistant-process-inline">
      {segments}
      {parts
        .slice(cutoff)
        .map((part, index) => renderPart(part, index + cutoff))}
    </div>
  )
}

function ProcessHistorySegment({
  parts,
  start,
  renderPart,
}: {
  parts: AdaptedContentPart[]
  start: number
  renderPart: (part: AdaptedContentPart, index: number) => ReactNode
}) {
  const t = useTranslations("ConversationPresentation")
  const id = useId()
  const { rootRef, open, onOpenChange } = useArchiveDisclosure(true)
  const label = useArchiveSummary(parts) || t("processRecord")
  return (
    <div ref={rootRef} className="assistant-process-inline">
      <button
        type="button"
        className="assistant-history-toggle"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => onOpenChange(!open)}
      >
        <History aria-hidden="true" className="size-3.5 shrink-0" />
        <span className="min-w-0 truncate" title={label}>
          {label}
        </span>
        <span className="shrink-0 text-xs">
          · {start + 1}–{start + parts.length}
        </span>
        <ProcessArchiveStatus parts={parts} />
        <ChevronRight
          aria-hidden="true"
          className={"size-3 shrink-0 " + (open ? "rotate-90" : "")}
        />
      </button>
      <AssistantFoldContent open={open} contentId={id}>
        <div className="assistant-process-inline">
          {parts.map((part, index) => renderPart(part, index + start))}
        </div>
      </AssistantFoldContent>
    </div>
  )
}
