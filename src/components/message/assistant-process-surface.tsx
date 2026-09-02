"use client"

import { useEffect, useRef, useState } from "react"
import { ArrowDown, CheckCircle2, ChevronDown, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Shimmer } from "@/components/ai-elements/shimmer"
import { ContentPartsRenderer } from "@/components/message/content-parts-renderer"
import { OnceEntrance } from "@/components/message/message-entrance"
import { Badge } from "@/components/ui/badge"
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

const COMPLETE_SETTLE_MS = 500

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
  if (part.type === "tool-call") return `tool-${part.toolCallId}`
  if (part.type === "tool-result") return `result-${part.toolCallId}`
  if (part.type === "goal-run") return `goal-${part.start.toolCallId}`
  return `${part.type}-${index}`
}

function CompletedLabel({
  durationMs,
  processCount,
}: {
  durationMs?: number | null
  processCount: number
}) {
  const t = useTranslations("Folder.chat.messageList")
  const tLive = useTranslations("Folder.chat.liveTurnStats")
  const duration =
    typeof durationMs === "number" && durationMs > 0
      ? formatElapsedLabel(durationMs, tLive)
      : null
  return (
    <span className="inline-flex min-w-0 max-w-full flex-wrap items-center gap-1.5 break-words text-xs text-muted-foreground">
      <CheckCircle2 className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" />
      <span>
        {duration
          ? t("processCompleted", { duration })
          : t("processCompletedWithoutDuration")}
      </span>
      <span className="text-muted-foreground/70">
        {t("processCount", { count: processCount })}
      </span>
    </span>
  )
}

function ProcessRows({
  parts,
  entranceKey,
  animationEnabled,
  conversationId,
  streaming,
}: Pick<
  AssistantProcessSurfaceProps,
  "parts" | "entranceKey" | "animationEnabled" | "conversationId"
> & { streaming: boolean }) {
  return parts.map((part, index) => {
    const key = processPartKey(part, index)
    return (
      <OnceEntrance
        key={key}
        entranceKey={`${entranceKey}:process:${key}`}
        animate={animationEnabled && streaming}
        offset={3}
        duration={180}
      >
        <ContentPartsRenderer
          parts={[part]}
          role="assistant"
          conversationId={conversationId}
          entranceKey={`${entranceKey}:process:${key}`}
          animationEnabled={animationEnabled}
          reasoningPresentation="inline"
        />
      </OnceEntrance>
    )
  })
}

export function AssistantProcessSurface(props: AssistantProcessSurfaceProps) {
  const t = useTranslations("Folder.chat.messageList")
  const tReasoning = useTranslations("Folder.chat.reasoning")
  const keepCompletedOpen =
    !props.collapseCompletedTurn ||
    props.displayMode === "full" ||
    (props.autoOpenErrors && props.processHasError)
  const [open, setOpen] = useState(
    !props.isResponseComplete || keepCompletedOpen
  )
  const previousComplete = useRef(props.isResponseComplete)
  const { handleScroll, isFollowing, scrollToLatest, viewportRef } =
    useProcessAutoFollow(props.parts, open)

  useEffect(() => {
    const justCompleted = !previousComplete.current && props.isResponseComplete
    previousComplete.current = props.isResponseComplete
    const nextOpen = !props.isResponseComplete || keepCompletedOpen
    const delay = justCompleted && !nextOpen ? COMPLETE_SETTLE_MS : 0
    const timer = window.setTimeout(() => setOpen(nextOpen), delay)
    return () => window.clearTimeout(timer)
  }, [keepCompletedOpen, props.isResponseComplete])

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="assistant-process-surface"
    >
      <CollapsibleTrigger className="assistant-process-trigger group flex min-h-9 min-w-0 max-w-full flex-wrap items-center gap-2 text-left text-xs transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <ChevronDown
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform duration-200",
            open ? "rotate-0" : "-rotate-90"
          )}
        />
        <span className="min-w-0 flex-1" aria-live="polite">
          {props.isResponseComplete ? (
            <CompletedLabel
              durationMs={props.durationMs}
              processCount={props.processCount}
            />
          ) : (
            <span className="inline-flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
              <Loader2 className="size-3.5 shrink-0 animate-spin motion-reduce:animate-none" />
              <Shimmer as="span" duration={1} shineColor="var(--primary)">
                {tReasoning("thinking")}
              </Shimmer>
              {props.processCount > 0 && (
                <span className="text-muted-foreground/70">
                  {t("processCount", { count: props.processCount })}
                </span>
              )}
            </span>
          )}
        </span>
        {props.processHasError && (
          <Badge variant="destructive" className="h-5 shrink-0 text-[10px]">
            {t("processHasErrors")}
          </Badge>
        )}
      </CollapsibleTrigger>
      <CollapsibleContent className="assistant-process-content">
        <div className="relative">
          <div
            ref={viewportRef}
            onScroll={handleScroll}
            className="assistant-process-viewport assistant-process-details max-h-[min(18rem,40vh)] overflow-y-auto overscroll-contain pe-2"
          >
            <div className="space-y-3">
              <ProcessRows
                parts={props.parts}
                entranceKey={props.entranceKey}
                animationEnabled={props.animationEnabled}
                conversationId={props.conversationId}
                streaming={!props.isResponseComplete}
              />
            </div>
          </div>
          {!isFollowing && (
            <button
              type="button"
              onClick={scrollToLatest}
              className="absolute bottom-2 right-2 inline-flex h-7 items-center gap-1 rounded-full border bg-background/95 px-2 text-[11px] text-muted-foreground shadow-sm transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <ArrowDown className="size-3" />
              {t("backToLatest")}
            </button>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
