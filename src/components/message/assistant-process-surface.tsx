"use client"

import { useId, type Ref } from "react"
import { MotionConfig } from "motion/react"
import { Virtualizer } from "virtua"
import { ArrowDown, Check, ChevronRight, Clock3 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Collapsible, CollapsibleContent } from "@/components/ui/collapsible"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { cn } from "@/lib/utils"
import { useProcessDisclosure } from "./process-disclosure"
import { completedProcessOperations } from "./process-operation-count"
import { ToolDisclosureScope } from "./tool-disclosure-context"
import {
  processDisplayParts,
  processPartNeedsAttention,
} from "./process-presentation"
import { useProcessArchive } from "./use-process-archive"
import { AssistantProcessRow } from "./assistant-process-row"
import { useProcessAutoFollow } from "./use-process-auto-follow"

const PROCESS_OVERSCAN = 200

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
  archived,
  streaming,
}: {
  durationMs?: number | null
  archived: number
  streaming: boolean
}) {
  const t = useTranslations("ConversationPresentation")
  const live = useTranslations("Folder.chat.liveTurnStats")
  const duration =
    typeof durationMs === "number" && durationMs > 0
      ? formatElapsedLabel(durationMs, live)
      : null
  return (
    <span className="inline-flex min-w-0 flex-wrap gap-x-2">
      {streaming ? (
        <>
          <span>{t("processRecord")}</span>
          {archived > 0 && (
            <span className="text-muted-foreground/70">
              {t("archivedProcessSegments", { count: archived })}
            </span>
          )}
        </>
      ) : (
        <span>
          {duration
            ? t("completedWithDuration", { duration })
            : t("completedWithoutDuration")}
        </span>
      )}
    </span>
  )
}

function ProcessRows(
  props: Pick<
    AssistantProcessSurfaceProps,
    "parts" | "entranceKey" | "animationEnabled"
  > & {
    streaming: boolean
    hiddenBefore: number
  }
) {
  const parts = processDisplayParts(props.parts)
  return parts.flatMap((part, index) => {
    if (index < props.hiddenBefore && !processPartNeedsAttention(part))
      return []
    const key = processPartKey(part, index)
    return (
      <AssistantProcessRow
        key={key}
        part={part}
        index={index}
        entranceKey={props.entranceKey}
        animationEnabled={props.animationEnabled}
        streaming={props.streaming}
      />
    )
  })
}

export function AssistantProcessSurface(props: AssistantProcessSurfaceProps) {
  const contentId = useId()
  const preferredOpen =
    (props.autoOpenErrors && props.processHasError) ||
    props.displayMode === "full" ||
    !props.collapseCompletedTurn
  const { open, onOpenChange, triggerRef, hasChoice } =
    useProcessDisclosure(preferredOpen)
  const { rootRef, reading, hiddenBefore } = useProcessArchive(
    props.parts,
    props.isResponseComplete,
    open
  )
  const visible =
    open ||
    !props.isResponseComplete ||
    props.parts.some(processPartNeedsAttention) ||
    (!hasChoice && reading)
  const completed = completedProcessOperations(props.parts)
  const archived = processDisplayParts(props.parts)
    .slice(0, hiddenBefore)
    .filter((part) => !processPartNeedsAttention(part)).length
  return (
    <Collapsible
      ref={rootRef}
      open={visible}
      className="assistant-process-surface"
    >
      <ToolDisclosureScope reading={reading}>
        <ProcessTrigger
          open={open}
          archived={props.isResponseComplete ? 0 : archived}
          completed={completed}
          props={props}
          triggerRef={triggerRef}
          onToggle={() => onOpenChange(!open)}
          contentId={contentId}
        />
        <CollapsibleContent
          id={contentId}
          className="assistant-process-content"
          inert={!visible}
        >
          <ProcessDetails
            {...props}
            hiddenBefore={hiddenBefore}
            reading={reading}
          />
        </CollapsibleContent>
      </ToolDisclosureScope>
    </Collapsible>
  )
}

function ProcessTrigger({
  open,
  archived,
  completed,
  props,
  triggerRef,
  onToggle,
  contentId,
}: {
  open: boolean
  archived: number
  completed: number
  props: AssistantProcessSurfaceProps
  triggerRef: Ref<HTMLButtonElement>
  onToggle: () => void
  contentId: string
}) {
  const t = useTranslations("ConversationPresentation")
  const streaming = !props.isResponseComplete
  const Icon = streaming
    ? archived > 0
      ? Check
      : Clock3
    : completed > 0 || props.isResponseComplete
      ? Check
      : Clock3
  return (
    <button
      type="button"
      aria-expanded={open}
      aria-controls={contentId}
      onClick={onToggle}
      ref={triggerRef}
      className="assistant-process-trigger group flex min-h-8 min-w-0 max-w-full flex-wrap items-center gap-2 text-left text-xs transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <Icon aria-hidden="true" className="size-3 shrink-0" />
      <ProcessLabel
        archived={archived}
        streaming={streaming}
        durationMs={props.isResponseComplete ? props.durationMs : null}
      />
      {(open || streaming) && (
        <span className="inline-flex items-center gap-1 text-muted-foreground">
          {t(open ? "collapseProcess" : "process")}
          <ChevronRight
            aria-hidden="true"
            className={cn(
              "size-3 shrink-0 transition-transform duration-100 motion-reduce:transition-none",
              open && "rotate-90"
            )}
          />
        </span>
      )}
      {!open && !streaming && (
        <ChevronRight aria-hidden="true" className="size-3 shrink-0" />
      )}
    </button>
  )
}

function ProcessDetails(
  props: AssistantProcessSurfaceProps & {
    hiddenBefore: number
    reading: boolean
  }
) {
  const t = useTranslations("ConversationPresentation")
  const messageList = useTranslations("Folder.chat.messageList")
  const { handleScroll, isFollowing, scrollToLatest, viewportRef } =
    useProcessAutoFollow(
      props.parts,
      !props.isResponseComplete && !props.reading
    )
  return (
    <div className="assistant-process-details relative min-w-0">
      <div
        ref={viewportRef}
        onScroll={handleScroll}
        className="assistant-process-scroll focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        role="region"
        aria-label={t("process")}
        tabIndex={0}
      >
        <MotionConfig reducedMotion="user">
          <Virtualizer scrollRef={viewportRef} bufferSize={PROCESS_OVERSCAN}>
            <ProcessRows
              {...props}
              streaming={!props.isResponseComplete}
              hiddenBefore={props.hiddenBefore}
            />
          </Virtualizer>
        </MotionConfig>
      </div>
      {!isFollowing && !props.isResponseComplete && (
        <button
          type="button"
          onClick={(event) => {
            scrollToLatest()
            event.currentTarget.blur()
          }}
          className="assistant-process-back-to-latest"
        >
          <ArrowDown aria-hidden="true" className="size-3" />
          {messageList("backToLatest")}
        </button>
      )}
    </div>
  )
}
