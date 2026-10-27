"use client"

import { useEffect, useId } from "react"
import { ChevronRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { useStickToBottom } from "use-stick-to-bottom"
import { ReasoningBody } from "@/components/ai-elements/reasoning-body"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useArchiveDisclosure } from "./use-archive-disclosure"

const PREVIEW_CHARACTER_LIMIT = 120

function ReasoningLabel({ content, open }: { content: string; open: boolean }) {
  const t = useTranslations("Folder.chat.reasoning")
  // 只预览末尾片段，让长段落持续输出时仍能看到最新进展。
  const preview = content.trimEnd().slice(-PREVIEW_CHARACTER_LIMIT)
  return (
    <>
      <span className="assistant-reasoning-title">{t("deepThinking")}</span>
      {content.trim() && (
        <ChevronRight
          aria-hidden="true"
          className={open ? "rotate-90" : undefined}
        />
      )}
      {!open && preview && (
        <span className="assistant-reasoning-preview" aria-hidden="true">
          {preview.replace(/\s+/g, " ").trim()}
        </span>
      )}
    </>
  )
}

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
  const { rootRef, reading, open, onOpenChange } = useArchiveDisclosure(true)
  const { scrollRef, contentRef } = useReasoningViewport(streaming, reading)
  const hasContent = Boolean(content.trim())
  return (
    <div ref={rootRef} className="assistant-ordered-reasoning">
      <button
        type="button"
        className="assistant-reasoning-label"
        disabled={!hasContent}
        aria-label={t("deepThinking")}
        aria-expanded={hasContent && open}
        aria-controls={id}
        onClick={() => onOpenChange(!open)}
      >
        <ReasoningLabel content={content} open={open} />
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
