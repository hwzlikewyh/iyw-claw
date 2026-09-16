"use client"

import { useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { Loader2, MessagesSquare, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { MessageResponse } from "@/components/ai-elements/message"
import { useCollabTranscript } from "@/hooks/use-collab-transcript"
import {
  classifyCollabStatus,
  shortAgentId,
  type CollabAgentState,
} from "@/lib/collab-tool"
import {
  COLLAB_STATUS_LABEL,
  collabActionDetail,
  collabTranscriptSummary,
  isCollabActive,
} from "@/lib/collab-presentation"
import { getToolDisplayName } from "@/lib/tool-display"
import { CollabTranscriptDialog } from "./collab-transcript-dialog"

export function CollabAgentRow({ agent }: { agent: CollabAgentState }) {
  const t = useTranslations("Folder.chat.collabAgent")
  const [open, setOpen] = useState(false)
  const record = useCollabTranscript(
    agent.threadId,
    isCollabActive(agent.status)
  )
  const summary = collabTranscriptSummary(record.detail)
  const name = agent.name || shortAgentId(agent.threadId)
  return (
    <section className="min-w-0 space-y-2 border-t border-border/60 pt-2 first:border-0 first:pt-0">
      <CollabRowHeader
        agent={agent}
        open={() => setOpen(true)}
        refresh={record.refresh}
      />
      {agent.task && (
        <p
          className="line-clamp-2 whitespace-pre-wrap break-words text-xs"
          title={agent.task}
        >
          {agent.task}
        </p>
      )}
      <CollabRecentActivity
        action={summary.lastAction}
        loading={record.loading}
      />
      <CollabResult
        status={agent.status}
        result={agent.message || summary.text}
      />
      {record.error && (
        <p className="break-words text-xs text-muted-foreground">
          {t("transcriptUnavailable")}: {record.error}
        </p>
      )}
      {open && (
        <CollabTranscriptDialog
          open={open}
          onOpenChange={setOpen}
          detail={record.detail}
          name={name}
        />
      )}
    </section>
  )
}

function CollabResult({
  status,
  result,
}: {
  status: string | null
  result: string | null
}) {
  const t = useTranslations("Folder.chat.collabAgent")
  if (!result) return null
  return (
    <div className="min-w-0 break-words text-xs">
      <div className="mb-1 font-medium text-muted-foreground">
        {t(status === "completed" ? "resultLabel" : "messageLabel")}
      </div>
      <MessageResponse>{result}</MessageResponse>
    </div>
  )
}

function CollabRowHeader({
  agent,
  open,
  refresh,
}: {
  agent: CollabAgentState
  open: () => void
  refresh: () => void
}) {
  const t = useTranslations("Folder.chat.collabAgent")
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <span
        className="min-w-0 flex-1 truncate font-mono text-xs"
        title={agent.threadId}
      >
        {agent.name || shortAgentId(agent.threadId)}
      </span>
      <span className="shrink-0 text-xs text-muted-foreground">
        {t(COLLAB_STATUS_LABEL[classifyCollabStatus(agent.status)])}
      </span>
      <TranscriptButton label={t("refreshTranscript")} onClick={refresh}>
        <RefreshCw />
      </TranscriptButton>
      <TranscriptButton label={t("openTranscript")} onClick={open}>
        <MessagesSquare />
      </TranscriptButton>
    </div>
  )
}

function CollabRecentActivity({
  action,
  loading,
}: {
  action: ReturnType<typeof collabTranscriptSummary>["lastAction"]
  loading: boolean
}) {
  const t = useTranslations("Folder.chat.collabAgent")
  const tools = useTranslations("Folder.chat.contentParts")
  const label =
    action?.type === "tool_use"
      ? getToolDisplayName(
          { toolName: action.tool_name, input: action.input_preview },
          (key) => {
            const messageKey = key as Parameters<typeof tools>[0]
            return tools.has(messageKey) ? tools(messageKey) : null
          }
        )
      : action?.type === "text"
        ? action.text
        : null
  const detail =
    action?.type === "tool_use"
      ? collabActionDetail(action.input_preview)
      : null
  return (
    <div className="flex min-w-0 items-start gap-1.5 text-xs text-muted-foreground">
      {loading && <Loader2 className="size-3.5 shrink-0 animate-spin" />}
      <span className="shrink-0">{t("recentActivity")}:</span>
      <span className="min-w-0 whitespace-pre-wrap break-words line-clamp-3">
        {label || t(loading ? "transcriptLoading" : "transcriptPending")}
        {detail ? `: ${detail}` : ""}
      </span>
    </div>
  )
}

function TranscriptButton({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label={label}
          onClick={onClick}
        >
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
