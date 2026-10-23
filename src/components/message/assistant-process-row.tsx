"use client"

import { useMemo } from "react"
import { Check, ChevronRight, CircleAlert, Clock3 } from "lucide-react"
import { useTranslations } from "next-intl"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { getToolDisplayName } from "@/lib/tool-display"
import { normalizeToolName } from "@/lib/tool-call-normalization"
import { cn } from "@/lib/utils"
import { buildBackgroundTaskRows } from "@/lib/background-task"
import { buildDelegationTaskRows } from "@/lib/delegation-status"
import {
  processPartHasFailure,
  processPartNeedsAttention,
} from "./process-presentation"
import { useToolDisclosure } from "./tool-disclosure-context"

interface AssistantProcessRowProps {
  part: AdaptedContentPart
  index: number
  streaming: boolean
  entranceKey: string
  animationEnabled: boolean
}

function toolLabel(
  part: Extract<AdaptedContentPart, { type: "tool-call" }>,
  translate: ReturnType<typeof useTranslations>
) {
  return getToolDisplayName(
    { toolName: part.toolName, displayTitle: part.displayTitle },
    (key, values) =>
      translate.has(key as never)
        ? translate(key as never, values as never)
        : null
  )
}

function partLabel(
  part: AdaptedContentPart,
  translate: ReturnType<typeof useTranslations>
): string {
  if (part.type === "tool-call") return toolLabel(part, translate)
  if (part.type === "tool-result") return translate("processStep.toolResult")
  if (part.type === "reasoning") return translate("processStep.thinking")
  if (part.type === "goal-run") return translate("processStep.task")
  if (part.type === "background-task-group")
    return translate("processStep.background")
  if (part.type === "delegation-status-group")
    return translate("processStep.delegation")
  if (part.type === "generated-image") return translate("processStep.image")
  if (part.type === "plan") return translate("processStep.plan")
  if (part.type === "plugin-app") return translate("processStep.plugin")
  return translate("processStep.tool")
}

function stepDescription(
  part: AdaptedContentPart,
  translate: ReturnType<typeof useTranslations>
): string {
  if (part.type === "reasoning") return translate("processStep.reasoningDetail")
  if (part.type === "tool-result") return translate("processStep.resultDetail")
  if (part.type === "tool-call") {
    const normalized = normalizeToolName(part.toolName)
    if (normalized === "bash" || normalized === "exec_command")
      return translate("processStep.commandDetail")
    if (normalized === "read" || normalized === "glob" || normalized === "grep")
      return translate("processStep.readDetail")
    if (normalized === "websearch" || normalized === "webfetch")
      return translate("processStep.webDetail")
    return translate("processStep.toolDetail")
  }
  return translate("processStep.genericDetail")
}

function rowStatus(
  part: AdaptedContentPart,
  streaming: boolean,
  translate: ReturnType<typeof useTranslations>
) {
  if (processPartHasFailure(part)) {
    return { label: translate("processStep.failed"), kind: "error" as const }
  }
  if (isPartRunning(part, streaming)) {
    return { label: translate("processStep.running"), kind: "running" as const }
  }
  if (processPartNeedsAttention(part))
    return { label: translate("processStep.waiting"), kind: "waiting" as const }
  return {
    label: translate("processStep.completed"),
    kind: "completed" as const,
  }
}

function isPartRunning(part: AdaptedContentPart, streaming: boolean): boolean {
  if (part.type === "tool-call")
    return (
      streaming &&
      (part.state === "input-available" || part.state === "input-streaming")
    )
  if (part.type === "reasoning") return streaming && part.isStreaming
  if (part.type === "goal-run") return streaming && part.isRunning
  if (part.type === "background-task-group")
    return buildBackgroundTaskRows(part.polls).some(
      (row) => row.badge === "running" || row.isInFlight
    )
  if (part.type === "delegation-status-group")
    return buildDelegationTaskRows(part.polls).some(
      (row) => row.badge.status === "running"
    )
  return false
}

export function AssistantProcessRow({
  part,
  index,
  streaming,
  entranceKey,
  animationEnabled,
}: AssistantProcessRowProps) {
  const content = useTranslations("Folder.chat.contentParts")
  const label = useMemo(() => partLabel(part, content), [part, content])
  const detail = useMemo(() => stepDescription(part, content), [part, content])
  const status = rowStatus(part, streaming, content)
  const isCurrent = streaming && processPartNeedsAttention(part)
  const detailId = `${entranceKey}:detail:${index}`
  const disclosureId = part.type === "tool-call" ? part.toolCallId : detailId
  const [expanded, setExpanded] = useToolDisclosure(disclosureId, false)
  return (
    <div
      className={cn(
        "assistant-process-row",
        animationEnabled && "assistant-process-row-enter",
        isCurrent && "assistant-process-row-current"
      )}
    >
      <button
        type="button"
        className="assistant-process-row-trigger"
        aria-expanded={expanded}
        aria-controls={detailId}
        onClick={() => setExpanded(!expanded)}
      >
        <ChevronRight
          aria-hidden="true"
          className={cn(
            "assistant-process-row-chevron",
            expanded && "rotate-90"
          )}
        />
        <span className="assistant-process-row-label">{label}</span>
        <span
          className={cn("assistant-process-row-status", `is-${status.kind}`)}
        >
          {status.kind === "completed" && <Check aria-hidden="true" />}
          {status.kind === "running" && <Clock3 aria-hidden="true" />}
          {status.kind === "waiting" && <Clock3 aria-hidden="true" />}
          {status.kind === "error" && <CircleAlert aria-hidden="true" />}
          {status.label}
        </span>
      </button>
      <div
        id={detailId}
        className="assistant-process-row-detail"
        hidden={!expanded}
      >
        {detail}
      </div>
    </div>
  )
}
