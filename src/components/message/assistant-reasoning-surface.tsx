"use client"

import { memo } from "react"
import { useTranslations } from "next-intl"
import { ArrowDown, BrainIcon, ChevronRightIcon } from "lucide-react"

import {
  Reasoning,
  ReasoningTrigger,
  useReasoning,
} from "@/components/ai-elements/reasoning"
import { ReasoningContent } from "@/components/ai-elements/reasoning"
import { Shimmer } from "@/components/ai-elements/shimmer"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { cn } from "@/lib/utils"

import { useProcessAutoFollow } from "./use-process-auto-follow"

type ReasoningPart = Extract<AdaptedContentPart, { type: "reasoning" }>

interface AssistantReasoningSurfaceProps {
  parts: ReasoningPart[]
  isResponseComplete: boolean
}

function ReasoningLabel() {
  const t = useTranslations("Folder.chat.reasoning")
  const { isStreaming, isOpen } = useReasoning()
  return (
    <ReasoningTrigger
      className="assistant-reasoning-trigger w-fit gap-1.5 px-0 py-1 text-xs"
      aria-label={t("deepThinking")}
    >
      <BrainIcon className="size-3.5 shrink-0" aria-hidden="true" />
      {isStreaming ? (
        <Shimmer as="span" duration={1} shineColor="var(--primary)">
          {t("deepThinking")}
        </Shimmer>
      ) : (
        <span>{t("deepThinking")}</span>
      )}
      <ChevronRightIcon
        className={cn(
          "size-3.5 shrink-0 transition-transform duration-200 motion-reduce:transition-none",
          isOpen && "rotate-90"
        )}
        aria-hidden="true"
      />
    </ReasoningTrigger>
  )
}

function ReasoningContentWithFollow({ content }: { content: string }) {
  const t = useTranslations("Folder.chat.messageList")
  const { isOpen } = useReasoning()
  const { handleScroll, isFollowing, scrollToLatest, viewportRef } =
    useProcessAutoFollow(content, isOpen)

  return (
    <div className="relative">
      <ReasoningContent
        ref={viewportRef}
        onScroll={handleScroll}
        className="assistant-reasoning-content"
      >
        {content}
      </ReasoningContent>
      {!isFollowing && isOpen && (
        <button
          type="button"
          onClick={scrollToLatest}
          className="assistant-reasoning-back-to-latest"
          aria-label={t("backToLatest")}
          title={t("backToLatest")}
        >
          <ArrowDown className="size-3" aria-hidden="true" />
        </button>
      )}
    </div>
  )
}

export const AssistantReasoningSurface = memo(
  function AssistantReasoningSurface({
    parts,
    isResponseComplete,
  }: AssistantReasoningSurfaceProps) {
    const content = parts
      .map((part) => part.content.trim())
      .filter(Boolean)
      .join("\n\n")
    if (!content && isResponseComplete) return null

    return (
      <Reasoning
        className="assistant-reasoning-surface"
        isStreaming={!isResponseComplete}
        defaultOpen={!isResponseComplete}
        expandable={Boolean(content) || !isResponseComplete}
      >
        <ReasoningLabel />
        {(content || !isResponseComplete) && (
          <ReasoningContentWithFollow content={content} />
        )}
      </Reasoning>
    )
  }
)
