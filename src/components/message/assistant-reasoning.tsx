"use client"

import { useEffect, useId } from "react"
import { ChevronRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { useStickToBottom } from "use-stick-to-bottom"
import { ReasoningBody } from "@/components/ai-elements/reasoning-body"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useArchiveDisclosure } from "./use-archive-disclosure"

function useReasoningViewport(streaming: boolean, reading: boolean) {
  const { scrollRef, contentRef, stopScroll } = useStickToBottom({
    initial: streaming ? "instant" : false,
    resize: "instant",
  })
  useEffect(() => {
    if (reading || !streaming) stopScroll()
  }, [reading, streaming, stopScroll])
  return { scrollRef, contentRef }
}

export function AssistantReasoning({
  content,
  streaming,
}: {
  content: string
  streaming: boolean
}) {
  const t = useTranslations("Folder.chat.reasoning")
  const id = useId()
  const { rootRef, reading, open, onOpenChange } =
    useArchiveDisclosure(!streaming)
  const { scrollRef, contentRef } = useReasoningViewport(streaming, reading)
  const hasContent = Boolean(content.trim())
  return (
    <div ref={rootRef} className="assistant-ordered-reasoning">
      <button
        type="button"
        className="assistant-reasoning-label"
        disabled={!hasContent}
        aria-expanded={hasContent && open}
        aria-controls={id}
        onClick={() => onOpenChange(!open)}
      >
        {t("deepThinking")}
        {hasContent && (
          <ChevronRight
            aria-hidden="true"
            className={open ? "rotate-90" : undefined}
          />
        )}
      </button>
      <AssistantFoldContent open={hasContent && open} contentId={id}>
        <div
          ref={scrollRef}
          tabIndex={0}
          role="region"
          aria-label={t("deepThinking")}
          className="assistant-reasoning-viewport"
        >
          <div ref={contentRef}>
            <ReasoningBody>{content}</ReasoningBody>
          </div>
        </div>
      </AssistantFoldContent>
    </div>
  )
}
