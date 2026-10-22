"use client"

import { useId } from "react"
import { Check, ChevronDown, Clock3 } from "lucide-react"
import { useTranslations } from "next-intl"
import { ContentPartsRenderer } from "./content-parts-renderer"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDisplayMode } from "@/lib/conversation-display-preferences"
import { cn } from "@/lib/utils"
import { ToolDisclosureScope } from "./tool-disclosure-context"
import { useArchiveDisclosure } from "./use-archive-disclosure"
import { AssistantFoldContent } from "./assistant-fold-content"

interface AssistantProcessSurfaceProps {
  parts: AdaptedContentPart[]
  processCount?: number
  processHasError?: boolean
  entranceKey: string
  animationEnabled: boolean
  isResponseComplete: boolean
  displayMode: ConversationDisplayMode
  collapseCompletedTurn: boolean
  autoOpenErrors: boolean
  conversationId: number
  durationMs?: number | null
}

export function AssistantProcessSurface(props: AssistantProcessSurfaceProps) {
  const contentId = useId()
  const archive =
    props.isResponseComplete &&
    props.collapseCompletedTurn &&
    props.displayMode !== "full"
  const { rootRef, reading, open, onOpenChange } = useArchiveDisclosure(archive)
  return (
    <div ref={rootRef} className="assistant-process-surface">
      <ProcessToggle
        open={open}
        complete={props.isResponseComplete}
        contentId={contentId}
        onToggle={() => onOpenChange(!open)}
      />
      <AssistantFoldContent open={open} contentId={contentId}>
        <ToolDisclosureScope reading={reading}>
          <div className="assistant-process-details min-w-0 py-1">
            <ContentPartsRenderer
              parts={props.parts}
              role="assistant"
              conversationId={props.conversationId}
              entranceKey={props.entranceKey}
              animationEnabled={false}
              intelligentUiStreaming={!props.isResponseComplete}
            />
          </div>
        </ToolDisclosureScope>
      </AssistantFoldContent>
    </div>
  )
}

function ProcessToggle({
  open,
  complete,
  contentId,
  onToggle,
}: {
  open: boolean
  complete: boolean
  contentId: string
  onToggle: () => void
}) {
  const t = useTranslations("ConversationPresentation")
  const Icon = complete ? Check : Clock3
  return (
    <button
      type="button"
      className="assistant-process-trigger inline-flex min-h-8 max-w-full items-center gap-2 text-left text-xs text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      aria-expanded={open}
      aria-controls={contentId}
      onClick={onToggle}
    >
      <Icon aria-hidden="true" className="size-3.5 shrink-0" />
      <span>{t("processRecord")}</span>
      <span>{t(open ? "collapseProcess" : "process")}</span>
      <ChevronDown
        aria-hidden="true"
        className={cn(
          "size-3 transition-transform duration-300 motion-reduce:transition-none",
          !open && "-rotate-90"
        )}
      />
    </button>
  )
}
