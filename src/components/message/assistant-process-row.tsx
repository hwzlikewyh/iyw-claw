"use client"

import { useMemo } from "react"
import {
  AppWindow,
  BookOpen,
  ChevronRight,
  CircleAlert,
  Clock3,
  FilePenLine,
  FilePlus,
  FileText,
  FolderOpen,
  LoaderCircle,
  Search,
  SquareTerminal,
  Wrench,
} from "lucide-react"
import { useTranslations } from "next-intl"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { getToolDisplayName } from "@/lib/tool-display"
import { cn } from "@/lib/utils"
import {
  processPartHasFailure,
  processPartNeedsAttention,
} from "./process-presentation"
import { useToolDisclosure } from "./tool-disclosure-context"
import {
  processToolKind,
  processToolTitle,
  type ProcessToolKind,
} from "./assistant-tool-presentation"
import { processToolDetails } from "./assistant-tool-sources"
import { AssistantToolDetails } from "./assistant-tool-details"
import { AssistantFoldContent } from "./assistant-fold-content"

interface AssistantProcessRowProps {
  part: AdaptedContentPart
  index: number
  streaming: boolean
  entranceKey: string
  animationEnabled: boolean
}
const ICONS = {
  read: FileText,
  write: FilePlus,
  edit: FilePenLine,
  folder: FolderOpen,
  search: Search,
  fetch: AppWindow,
  command: SquareTerminal,
  skill: BookOpen,
  tool: Wrench,
}

function useRowPresentation(part: AdaptedContentPart, streaming: boolean) {
  const content = useTranslations("Folder.chat.contentParts")
  const activity = useTranslations("ConversationPresentation.toolActivity")
  return useMemo(() => {
    const tool = part.type === "tool-call" ? part : null
    const fallback = tool
      ? getToolDisplayName(tool, (key, values) =>
          content.has(key as never)
            ? content(key as never, values as never)
            : null
        )
      : content("processStep.toolResult")
    const kind: ProcessToolKind = tool ? processToolKind(tool) : "tool"
    const failed = processPartHasFailure(part)
    const pending = processPartNeedsAttention(part)
    const status = failed
      ? "failed"
      : pending
        ? streaming
          ? "running"
          : "waiting"
        : "completed"
    const label = tool
      ? processToolTitle(
          tool,
          (key, values) => activity(key as never, values as never),
          fallback
        )
      : fallback
    return {
      kind,
      status,
      statusLabel: content(("processStep." + status) as never),
      title:
        status === "running" ? activity("running", { target: label }) : label,
      details: tool ? processToolDetails(tool) : null,
    }
  }, [part, streaming, content, activity])
}

type RowPresentation = ReturnType<typeof useRowPresentation>
function ProcessRowHeader({
  row,
  open,
  contentId,
  onToggle,
}: {
  row: RowPresentation
  open: boolean
  contentId: string
  onToggle: () => void
}) {
  const Icon = ICONS[row.kind]
  return (
    <button
      type="button"
      className="assistant-process-row-trigger"
      disabled={!row.details}
      aria-expanded={row.details ? open : undefined}
      aria-controls={row.details ? contentId : undefined}
      aria-label={row.title + " · " + row.statusLabel}
      onClick={onToggle}
    >
      <Icon aria-hidden="true" className="assistant-process-row-icon" />
      <span className="assistant-process-row-label">{row.title}</span>
      {row.details && (
        <ChevronRight
          aria-hidden="true"
          className={cn("assistant-process-row-chevron", open && "rotate-90")}
        />
      )}
      {row.status !== "completed" && (
        <span
          className={cn("assistant-process-row-status", "is-" + row.status)}
        >
          {row.status === "failed" ? (
            <CircleAlert aria-hidden="true" />
          ) : row.status === "running" ? (
            <LoaderCircle aria-hidden="true" className="animate-spin" />
          ) : (
            <Clock3 aria-hidden="true" />
          )}
          {row.statusLabel}
        </span>
      )}
    </button>
  )
}

export function AssistantProcessRow(props: AssistantProcessRowProps) {
  const row = useRowPresentation(props.part, props.streaming)
  const id = props.entranceKey + ":detail:" + props.index
  const key = props.part.type === "tool-call" ? props.part.toolCallId : id
  const [expanded, setExpanded] = useToolDisclosure(key, false)
  return (
    <div className="assistant-process-row">
      <ProcessRowHeader
        row={row}
        open={expanded}
        contentId={id}
        onToggle={() => setExpanded(!expanded)}
      />
      {row.details && (
        <AssistantFoldContent open={expanded} contentId={id}>
          <AssistantToolDetails details={row.details} />
        </AssistantFoldContent>
      )}
    </div>
  )
}
