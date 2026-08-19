"use client"

import { Archive } from "lucide-react"
import { useTranslations } from "next-intl"

import type { ToolCallState } from "@/lib/adapters/ai-elements-adapter"
import { contextCompactionPayload } from "@/lib/context-compaction"

export { isContextCompactionMeta } from "@/lib/context-compaction"

function readNumber(
  source: Record<string, unknown> | null | undefined,
  key: string
): number | null {
  if (!source) return null
  const value = source[key]
  return typeof value === "number" && Number.isFinite(value) ? value : null
}

function readText(
  source: Record<string, unknown> | null | undefined,
  key: string
): string | null {
  if (!source) return null
  const value = source[key]
  return typeof value === "string" && value.trim().length > 0 ? value : null
}

function formatDuration(durationMs: number | null): string | null {
  if (durationMs === null || durationMs <= 0) return null
  const seconds = durationMs / 1000
  return `${seconds >= 10 ? Math.round(seconds) : seconds.toFixed(1)}s`
}

interface ContextCompactionCardProps {
  state?: ToolCallState
  meta?: Record<string, unknown> | null
}

export function ContextCompactionCard({
  state,
  meta,
}: ContextCompactionCardProps) {
  const t = useTranslations("Folder.chat.contentParts.contextCompaction")
  const payload = contextCompactionPayload(meta)
  const before =
    readNumber(meta, "tokensBefore") ?? readNumber(payload, "preTokens")
  const after =
    readNumber(meta, "tokensAfter") ?? readNumber(payload, "postTokens")
  const errorText = readText(payload, "error")
  const running = state === "input-streaming" || state === "input-available"
  const failed = errorText !== null || state === "output-error"
  const duration = formatDuration(readNumber(payload, "durationMs"))
  const tooltip = errorText ?? readText(payload, "trigger") ?? undefined
  const label = failed
    ? t("failed")
    : running
      ? t("compacting")
      : before !== null && after !== null && before !== after
        ? t("compactedTokens", {
            before: before.toLocaleString(),
            after: after.toLocaleString(),
          })
        : t("compacted")

  return (
    <div className="flex items-center gap-3 py-1 text-xs text-muted-foreground/80 select-none">
      <div className="h-px flex-1 bg-gradient-to-r from-transparent to-border/70" />
      <div
        className={`flex shrink-0 items-center gap-1.5${failed ? " text-destructive/80" : ""}`}
        title={tooltip}
      >
        <Archive className="size-3.5" />
        <span className={running ? "animate-pulse" : undefined}>{label}</span>
        {!failed && !running && duration ? (
          <span className="text-muted-foreground/60">· {duration}</span>
        ) : null}
      </div>
      <div className="h-px flex-1 bg-gradient-to-l from-transparent to-border/70" />
    </div>
  )
}
