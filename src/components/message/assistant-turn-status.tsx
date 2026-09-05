"use client"

import { AlertCircle, CheckCircle2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { AgentIcon } from "@/components/agent-icon"
import { formatElapsedLabel } from "@/lib/format-elapsed"
import type { AgentType } from "@/lib/types"

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
