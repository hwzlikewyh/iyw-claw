"use client"

import { useId, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { ChevronRight, History } from "lucide-react"
import type { AssistantArchiveEntry } from "./assistant-archive-segments"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import { assistantResponseFirstLine } from "./assistant-text-segments"
import { ProcessArchiveStatus } from "./assistant-process-history"

type Group = Extract<AssistantArchiveEntry, { kind: "group" }>

export function AssistantProgressSegment({
  entry,
  fold,
  children,
}: {
  entry: Group
  fold: boolean
  children: ReactNode
}) {
  const t = useTranslations("ConversationPresentation")
  const id = useId()
  const archive = fold && entry.settled
  const { rootRef, open, onOpenChange } = useArchiveDisclosure(archive)
  const text = entry.items.flatMap(({ section }) =>
    section.parts.flatMap((part) => (part.type === "text" ? [part.text] : []))
  )
  const label = assistantResponseFirstLine(text) || t("processRecord")
  const tools = entry.items
    .flatMap((item) => item.section.parts)
    .filter((part) => part.type === "tool-call")
  const count = tools.length
  return (
    <div
      ref={rootRef}
      className="assistant-progress-segment"
      data-archived={archive}
    >
      {archive && (
        <button
          type="button"
          className="assistant-progress-toggle"
          aria-expanded={open}
          aria-controls={id}
          onClick={() => onOpenChange(!open)}
        >
          <History aria-hidden="true" className="size-3.5 shrink-0" />
          <span className="min-w-0 truncate">{label}</span>
          {count > 0 && (
            <span className="shrink-0 text-xs">
              {t("completedOperations", { count })}
            </span>
          )}
          <ProcessArchiveStatus parts={tools} />
          <ChevronRight
            aria-hidden="true"
            className={"size-3 shrink-0 " + (open ? "rotate-90" : "")}
          />
        </button>
      )}
      <AssistantFoldContent open={!archive || open} contentId={id}>
        {children}
      </AssistantFoldContent>
    </div>
  )
}
