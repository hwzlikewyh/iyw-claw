"use client"

import { Check, Copy } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import type { ReactNode } from "react"
import { AgentIcon } from "@/components/agent-icon"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { formatConversationTitle } from "@/lib/conversation-title"
import { copyTextToClipboard } from "@/lib/utils"
import {
  STATUS_ORDER,
  type DbConversationSummary,
  type ConversationStatus,
} from "@/lib/types"
import { useCopiedFlag } from "@/hooks/use-copied-flag"
import { ConversationStatusDot } from "./conversation-status-dot"

export function SessionIdentity({
  summary,
  model,
}: {
  summary: DbConversationSummary
  model: string | null
}) {
  const t = useTranslations("Folder.sessionDetails")
  const statusT = useTranslations("Folder.statusLabels")
  const known = (STATUS_ORDER as string[]).includes(summary.status)
  return (
    <div className="min-w-0 space-y-2.5 border-b pb-4">
      <p
        className="line-clamp-2 break-words text-sm font-medium"
        title={formatConversationTitle(summary.title) || t("untitled")}
      >
        {formatConversationTitle(summary.title) || t("untitled")}
      </p>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted-foreground">
        <span className="inline-flex items-center gap-1.5">
          <AgentIcon agentType={summary.agent_type} className="size-3.5" />
          {getAgentDisplayName(summary.agent_type)}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <ConversationStatusDot
            size="sm"
            status={known ? (summary.status as ConversationStatus) : null}
          />
          {known
            ? statusT(summary.status as ConversationStatus)
            : summary.status}
        </span>
        {model && <span className="break-all">{model}</span>}
      </div>
    </div>
  )
}

function MetadataField({
  label,
  children,
}: {
  label: string
  children: ReactNode
}) {
  return (
    <div className="min-w-0 space-y-1">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="break-words text-xs leading-5">{children}</dd>
    </div>
  )
}

function CopyableIdentifier({
  value,
  label,
}: {
  value: string
  label: string
}) {
  const t = useTranslations("Folder.sessionDetails")
  const [copied, markCopied] = useCopiedFlag()
  const copy = () =>
    void copyTextToClipboard(value).then((ok) => {
      if (ok) markCopied()
    })
  return (
    <span className="inline-flex min-w-0 max-w-full items-start gap-1">
      <span className="break-all font-mono">{value}</span>
      <button
        type="button"
        onClick={copy}
        title={
          copied
            ? t("copiedField", { field: label })
            : t("copyField", { field: label })
        }
        aria-label={t("copyField", { field: label })}
        className="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {copied ? (
          <Check className="size-3 text-emerald-600" />
        ) : (
          <Copy className="size-3" />
        )}
      </button>
    </span>
  )
}

export function SessionMetadata({
  summary,
}: {
  summary: DbConversationSummary
}) {
  const t = useTranslations("Folder.sessionDetails")
  const locale = useLocale()
  const formatDate = (value: string) => {
    const date = new Date(value)
    return Number.isNaN(date.getTime())
      ? value
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(date)
  }
  return (
    <dl className="grid grid-cols-2 gap-x-5 gap-y-3 pt-4">
      <MetadataField label={t("sessionId")}>
        <CopyableIdentifier value={String(summary.id)} label={t("sessionId")} />
      </MetadataField>
      <MetadataField label={t("gitBranch")}>
        {summary.git_branch || t("none")}
      </MetadataField>
      <MetadataField label={t("createdAt")}>
        {formatDate(summary.created_at)}
      </MetadataField>
      <MetadataField label={t("updatedAt")}>
        {formatDate(summary.updated_at)}
      </MetadataField>
      {summary.parent_id != null && (
        <MetadataField label={t("parentId")}>{summary.parent_id}</MetadataField>
      )}
      <div className="col-span-2">
        <MetadataField label={t("externalId")}>
          {summary.external_id ? (
            <CopyableIdentifier
              value={summary.external_id}
              label={t("externalId")}
            />
          ) : (
            t("none")
          )}
        </MetadataField>
      </div>
    </dl>
  )
}
