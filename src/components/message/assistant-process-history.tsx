"use client"

import { useId, type ReactNode } from "react"
import { ChevronRight, History } from "lucide-react"
import { useTranslations } from "next-intl"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import {
  processPartHasFailure,
  processPartNeedsAttention,
} from "./process-presentation"

const HISTORY_THRESHOLD = 8
const VISIBLE_RECENT_ACTIONS = 4

export function ProcessArchiveStatus({
  parts,
}: {
  parts: AdaptedContentPart[]
}) {
  const t = useTranslations("Folder.chat.contentParts.processStep")
  const failures = parts.filter(processPartHasFailure).length
  const running = parts.filter(
    (part) => !processPartHasFailure(part) && processPartNeedsAttention(part)
  ).length
  return (
    <>
      {failures > 0 && (
        <span className="assistant-history-status is-failed">
          {t("failed")} · {failures}
        </span>
      )}
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
  const t = useTranslations("ConversationPresentation")
  const id = useId()
  const archive = enabled && parts.length >= HISTORY_THRESHOLD
  const { rootRef, open, onOpenChange } = useArchiveDisclosure(archive)
  const cutoff = archive ? parts.length - VISIBLE_RECENT_ACTIONS : 0
  const previous = parts.slice(0, cutoff)
  return (
    <div ref={rootRef} className="assistant-process-inline">
      {archive && (
        <button
          type="button"
          className="assistant-history-toggle"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => onOpenChange(!open)}
        >
          <History aria-hidden="true" className="size-3.5 shrink-0" />
          <span>
            {t("processRecord")} · {previous.length}
          </span>
          <ProcessArchiveStatus parts={previous} />
          <ChevronRight
            aria-hidden="true"
            className={"size-3 shrink-0 " + (open ? "rotate-90" : "")}
          />
        </button>
      )}
      {archive && (
        <AssistantFoldContent open={open} contentId={id}>
          <div className="assistant-process-inline">
            {previous.map(renderPart)}
          </div>
        </AssistantFoldContent>
      )}
      {parts
        .slice(cutoff)
        .map((part, index) => renderPart(part, index + cutoff))}
    </div>
  )
}
