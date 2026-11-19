"use client"

import { AlertCircle, CheckCircle2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useMemo } from "react"

import { AgentIcon } from "@/components/agent-icon"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import type { AgentType } from "@/lib/types"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { isImageGenerationToolName } from "@/lib/image-delivery"
import {
  imageGenerationErrors,
  type ImageErrorLabel,
} from "@/lib/image-generation-errors"
import { flattenAssistantParts } from "./assistant-turn-timeline"

export function AssistantIdentity({ agentType }: { agentType: AgentType }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div className="flex items-center gap-2 text-sm font-semibold">
      <AgentIcon
        agentType={agentType}
        className="size-6 rounded-full bg-muted p-0.5"
      />
      <span>{t("assistantName")}</span>
    </div>
  )
}

export function CompletedProcessSummary({
  durationMs,
  processCount,
  hasError = false,
}: {
  durationMs?: number | null
  processCount: number
  hasError?: boolean
}) {
  const t = useTranslations("Folder.chat.messageList")
  const tLive = useTranslations("Folder.chat.liveTurnStats")
  const duration =
    typeof durationMs === "number" && durationMs > 0
      ? formatElapsedLabel(durationMs, tLive)
      : null
  const label = duration
    ? t("processCompleted", { duration })
    : t("processCompletedWithoutDuration")
  return (
    <span className="inline-flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      {hasError ? (
        <AlertCircle className="size-3.5 shrink-0 text-destructive" />
      ) : (
        <CheckCircle2 className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" />
      )}
      <span>{hasError ? t("processHasErrors") : label}</span>
      {processCount > 0 && (
        <span className="text-muted-foreground/70">
          {t("processCount", { count: processCount })}
        </span>
      )}
    </span>
  )
}

function partImageErrors(part: AdaptedContentPart): ImageErrorLabel[] {
  if (
    part.type === "generated-image" &&
    isImageGenerationToolName(part.sourceToolName ?? "")
  ) {
    if (part.status === "pending" || part.status === "in_progress") return []
    const failed = part.status === "failed"
    return imageGenerationErrors(
      part.sourceToolOutput ?? (failed ? part.revisedPrompt : null),
      failed
    )
  }
  if (
    part.type === "tool-call" &&
    isImageGenerationToolName(part.toolName) &&
    part.state.startsWith("output-")
  ) {
    return imageGenerationErrors(
      part.errorText ?? part.output,
      part.state === "output-error"
    )
  }
  return []
}

export function ImageGenerationNotices({
  parts,
}: {
  parts: AdaptedContentPart[]
}) {
  const t = useTranslations("Folder.chat.messageList")
  const labels = useMemo(
    () => [...new Set(flattenAssistantParts(parts).flatMap(partImageErrors))],
    [parts]
  )
  if (!labels.length) return null
  return (
    <div
      role="status"
      className="flex items-start gap-1.5 text-sm text-destructive"
    >
      <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      <div>
        {labels.map((label) => (
          <p key={label}>{t(label)}</p>
        ))}
      </div>
    </div>
  )
}

export function ImageArtifactRegistrationNotice({
  state,
}: {
  state: "failed" | "partial"
}) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div className="inline-flex items-center gap-1.5 rounded-md border border-destructive/30 bg-destructive/5 px-2.5 py-1.5 text-xs text-destructive">
      <AlertCircle className="size-3.5 shrink-0" />
      <span>
        {t(
          state === "failed"
            ? "imageArtifactRegistrationFailed"
            : "imageArtifactRegistrationPartial"
        )}
      </span>
    </div>
  )
}
