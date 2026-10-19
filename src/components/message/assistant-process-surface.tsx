"use client"

import { useState } from "react"
import { MotionConfig } from "motion/react"
import { AlertCircle, ArrowDown, ChevronDown } from "lucide-react"
import { useTranslations } from "next-intl"
import { ContentPartsRenderer } from "./content-parts-renderer"
import { OnceEntrance } from "./message-entrance"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { cn } from "@/lib/utils"
import { useProcessAutoFollow } from "./use-process-auto-follow"

interface AssistantProcessSurfaceProps {
  parts: AdaptedContentPart[]
  processCount: number
  processHasError: boolean
  entranceKey: string
  animationEnabled: boolean
  isResponseComplete: boolean
  displayMode: ConversationDisplayMode
  collapseCompletedTurn: boolean
  autoOpenErrors: boolean
  conversationId: number
  durationMs?: number | null
}

function processPartKey(part: AdaptedContentPart, index: number): string {
  if (part.type === "tool-call") return "tool-" + part.toolCallId
  if (part.type === "tool-result") return "result-" + part.toolCallId
  if (part.type === "goal-run") return "goal-" + part.start.toolCallId
  return part.type + "-" + index
}

function ProcessLabel({
  durationMs,
  processCount,
  hasError,
}: {
  durationMs?: number | null
  processCount: number
  hasError: boolean
}) {
  const t = useTranslations("ConversationPresentation")
  const message = useTranslations("Folder.chat.messageList")
  const live = useTranslations("Folder.chat.liveTurnStats")
  const duration =
    typeof durationMs === "number" && durationMs > 0
      ? formatElapsedLabel(durationMs, live)
      : null
  return (
    <span className="inline-flex min-w-0 flex-wrap gap-x-2">
      <span>{t("process")}</span>
      {hasError && (
        <span className="inline-flex items-center gap-1 text-destructive">
          <AlertCircle aria-hidden="true" className="size-3" />
          {message("processHasErrors")}
        </span>
      )}
      <span className="text-muted-foreground/70">{processCount}</span>
      {duration && <span className="text-muted-foreground/70">{duration}</span>}
    </span>
  )
}

function ProcessRows(
  props: Pick<
    AssistantProcessSurfaceProps,
    "parts" | "entranceKey" | "animationEnabled" | "conversationId"
  > & { streaming: boolean }
) {
  return props.parts.map((part, index) => {
    const key = processPartKey(part, index)
    const entranceKey = props.entranceKey + ":process:" + key
    return (
      <OnceEntrance
        key={key}
        entranceKey={entranceKey}
        animate={props.animationEnabled && props.streaming}
        offset={1}
        duration={120}
      >
        <ContentPartsRenderer
          parts={[part]}
          role="assistant"
          conversationId={props.conversationId}
          entranceKey={entranceKey}
          animationEnabled={props.animationEnabled}
          reasoningPresentation="inline"
          intelligentUiStreaming={props.streaming}
        />
      </OnceEntrance>
    )
  })
}

export function AssistantProcessSurface(props: AssistantProcessSurfaceProps) {
  const [chosenOpen, setChosenOpen] = useState<boolean | null>(null)
  const preferredOpen =
    props.displayMode === "full" ||
    (props.isResponseComplete &&
      (!props.collapseCompletedTurn ||
        (props.autoOpenErrors && props.processHasError)))
  const open = chosenOpen ?? preferredOpen
  return (
    <Collapsible
      open={open}
      onOpenChange={setChosenOpen}
      className="assistant-process-surface"
    >
      <CollapsibleTrigger className="assistant-process-trigger group flex min-h-8 min-w-0 max-w-full items-center gap-2 text-left text-xs transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <ChevronDown
          aria-hidden="true"
          className={cn(
            "size-3 shrink-0 transition-transform duration-100 motion-reduce:transition-none",
            !open && "-rotate-90"
          )}
        />
        <ProcessLabel
          hasError={props.processHasError}
          processCount={props.processCount}
          durationMs={props.isResponseComplete ? props.durationMs : null}
        />
      </CollapsibleTrigger>
      <CollapsibleContent className="assistant-process-content" inert={!open}>
        <ProcessDetails {...props} open={open} />
      </CollapsibleContent>
    </Collapsible>
  )
}

function ProcessDetails(
  props: AssistantProcessSurfaceProps & { open: boolean }
) {
  const t = useTranslations("Folder.chat.messageList")
  const { handleScroll, isFollowing, scrollToLatest, viewportRef } =
    useProcessAutoFollow(props.parts, props.open)
  return (
    <div className="relative">
      <div
        ref={viewportRef}
        onScroll={handleScroll}
        className="assistant-process-viewport assistant-process-details max-h-[min(18rem,40vh)] overflow-y-auto overscroll-contain pe-2"
      >
        <div className="space-y-3">
          <MotionConfig reducedMotion="always">
            <ProcessRows {...props} streaming={!props.isResponseComplete} />
          </MotionConfig>
        </div>
      </div>
      {!isFollowing && (
        <button
          type="button"
          onClick={scrollToLatest}
          className="absolute bottom-2 right-2 inline-flex h-7 items-center gap-1 rounded-full border bg-background/95 px-2 text-[11px] text-muted-foreground shadow-sm hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <ArrowDown aria-hidden="true" className="size-3" />
          {t("backToLatest")}
        </button>
      )}
    </div>
  )
}
