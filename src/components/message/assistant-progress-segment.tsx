"use client"

import { useId, useMemo, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { ChevronRight, History } from "lucide-react"
import type { AssistantArchiveEntry } from "./assistant-archive-segments"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import { useArchiveSummary } from "./use-archive-summary"
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
  const parts = useMemo(
    () => entry.items.flatMap(({ section }) => section.parts),
    [entry.items]
  )
  const label = useArchiveSummary(parts, archive)
  const tools = parts.filter((part) => part.type === "tool-call")
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
          <span className="min-w-0 truncate" title={label || undefined}>
            {label || t("processRecord")}
          </span>
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
