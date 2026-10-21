"use client"

import type { Ref, RefObject } from "react"
import { Virtualizer } from "virtua"
import { MotionConfig } from "motion/react"
import { useStickToBottom } from "use-stick-to-bottom"
import { Check, ChevronDown, ChevronUp, Clock3 } from "lucide-react"
import { useTranslations } from "next-intl"
import { ContentPartsRenderer } from "./content-parts-renderer"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import { cn } from "@/lib/utils"
import { useProcessDisclosure } from "./process-disclosure"
import { completedProcessOperations } from "./process-operation-count"
import { ToolDisclosureScope } from "./tool-disclosure-context"

const PROCESS_ROW_HEIGHT = 52
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
  completed,
}: {
  durationMs?: number | null
  completed: number
}) {
  const t = useTranslations("ConversationPresentation")
  const live = useTranslations("Folder.chat.liveTurnStats")
  const duration =
    typeof durationMs === "number" && durationMs > 0
      ? formatElapsedLabel(durationMs, live)
      : null
  return (
    <span className="inline-flex min-w-0 flex-wrap gap-x-2">
      <span>
        {completed > 0
          ? t("completedOperations", { count: completed })
          : t("processRecord")}
      </span>
      {duration && <span className="text-muted-foreground/70">{duration}</span>}
    </span>
  )
}

function ProcessRows(
  props: Pick<
    AssistantProcessSurfaceProps,
    "parts" | "entranceKey" | "animationEnabled" | "conversationId"
  > & { streaming: boolean; scrollRef: RefObject<HTMLElement | null> }
) {
  // 外层已负责收起过程，通用工具分组在这里直接展示步骤，避免二次展开。
  const parts = props.parts.flatMap<AdaptedContentPart>((part) =>
    part.type === "tool-group" ? part.items : [part]
  )
  const rows = parts.map((part, index) => {
    const key = processPartKey(part, index)
    const entranceKey = props.entranceKey + ":process:" + key
    return (
      <div key={key} className="pb-3">
        <ContentPartsRenderer
          parts={[part]}
          role="assistant"
          conversationId={props.conversationId}
          entranceKey={entranceKey}
          animationEnabled={false}
          reasoningPresentation="inline"
          intelligentUiStreaming={props.streaming}
        />
      </div>
    )
  })
  return (
    <ToolDisclosureScope>
      <Virtualizer
        scrollRef={props.scrollRef}
        itemSize={PROCESS_ROW_HEIGHT}
        bufferSize={PROCESS_OVERSCAN}
      >
        {rows}
      </Virtualizer>
    </ToolDisclosureScope>
  )
}

export function AssistantProcessSurface(props: AssistantProcessSurfaceProps) {
  const preferredOpen =
    (props.autoOpenErrors && props.processHasError) ||
    (props.isResponseComplete
      ? !props.collapseCompletedTurn && props.displayMode !== "minimal"
      : props.displayMode !== "minimal")
  const { open, onOpenChange, triggerRef } = useProcessDisclosure(preferredOpen)
  const completed = completedProcessOperations(props.parts)
  return (
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      className="assistant-process-surface"
    >
      <ProcessTrigger
        open={open}
        completed={completed}
        props={props}
        triggerRef={triggerRef}
      />
      <CollapsibleContent className="assistant-process-content" inert={!open}>
        <ProcessDetails {...props} onCollapse={() => onOpenChange(false)} />
      </CollapsibleContent>
    </Collapsible>
  )
}

function ProcessTrigger({
  open,
  completed,
  props,
  triggerRef,
}: {
  open: boolean
  completed: number
  props: AssistantProcessSurfaceProps
  triggerRef: Ref<HTMLButtonElement>
}) {
  const t = useTranslations("ConversationPresentation")
  const Icon = completed > 0 ? Check : Clock3
  return (
    <CollapsibleTrigger
      ref={triggerRef}
      className="assistant-process-trigger group flex min-h-8 min-w-0 max-w-full flex-wrap items-center gap-2 text-left text-xs transition-colors duration-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <Icon aria-hidden="true" className="size-3 shrink-0" />
      <ProcessLabel
        completed={completed}
        durationMs={props.isResponseComplete ? props.durationMs : null}
      />
      <span className="inline-flex items-center gap-1 text-muted-foreground">
        {t(open ? "collapseProcess" : "process")}
        <ChevronDown
          aria-hidden="true"
          className={cn(
            "size-3 shrink-0 transition-transform duration-100 motion-reduce:transition-none",
            !open && "-rotate-90"
          )}
        />
      </span>
    </CollapsibleTrigger>
  )
}

function ProcessDetails(
  props: AssistantProcessSurfaceProps & { onCollapse: () => void }
) {
  const t = useTranslations("ConversationPresentation")
  const { scrollRef, contentRef } = useStickToBottom({
    initial: props.isResponseComplete ? false : "instant",
    resize: "instant",
  })
  return (
    <div className="assistant-process-details min-w-0">
      <div
        ref={scrollRef}
        className="assistant-process-scroll focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        role="region"
        aria-label={t("process")}
        tabIndex={0}
      >
        <div ref={contentRef} className="space-y-3">
          <MotionConfig reducedMotion="always">
            <ProcessRows
              {...props}
              streaming={!props.isResponseComplete}
              scrollRef={scrollRef}
            />
          </MotionConfig>
        </div>
      </div>
      <button
        type="button"
        onClick={props.onCollapse}
        className="mt-3 inline-flex min-h-7 items-center gap-1 text-xs text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {t("collapseProcess")}
        <ChevronUp aria-hidden="true" className="size-3" />
      </button>
    </div>
  )
}
