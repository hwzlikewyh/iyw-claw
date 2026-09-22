"use client"
import { useTranslations } from "next-intl"
import {
  Compass,
  FileText,
  Globe,
  ListTodo,
  Search,
  Terminal,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { UnifiedDiffPreview } from "@/components/diff/unified-diff-preview"
import { MessageResponse } from "@/components/ai-elements/message"
import type { ParsedPermissionToolCall } from "@/lib/permission-request"

type DetailsProps = { parsed: ParsedPermissionToolCall }

export function PermissionDetails({ parsed }: DetailsProps) {
  const t = useTranslations("Folder.chat.permissionDialog")
  return (
    <div className="space-y-3 [overflow-wrap:anywhere]">
      {parsed.diffPreview && (
        <UnifiedDiffPreview diffText={parsed.diffPreview} />
      )}
      <PermissionPlan parsed={parsed} />
      <PermissionAllowedActions parsed={parsed} />
      {parsed.modeTarget && (
        <div className="flex items-start gap-2 text-xs text-muted-foreground">
          <Compass className="size-3.5 shrink-0" />
          <span>{t("targetMode", { mode: parsed.modeTarget })}</span>
        </div>
      )}
      <PermissionWeb parsed={parsed} />
      <PermissionFallback parsed={parsed} />
    </div>
  )
}

function PermissionPlan({ parsed }: DetailsProps) {
  const t = useTranslations("Folder.chat.permissionDialog")
  if (
    !parsed.planExplanation &&
    !parsed.planEntries.length &&
    !parsed.planMarkdown
  )
    return null
  return (
    <div className="space-y-2 text-xs leading-5">
      <div className="flex items-center gap-2 text-muted-foreground">
        {parsed.planMarkdown ? (
          <FileText className="size-3.5" />
        ) : (
          <ListTodo className="size-3.5" />
        )}
        <span>{t("plan")}</span>
      </div>
      {parsed.planExplanation && <p>{parsed.planExplanation}</p>}
      {parsed.planEntries.map((entry, index) => (
        <p key={index}>
          {entry.text}
          {entry.status && (
            <span className="ms-2 text-muted-foreground">({entry.status})</span>
          )}
        </p>
      ))}
      {parsed.planMarkdown && (
        <MessageResponse>{parsed.planMarkdown}</MessageResponse>
      )}
    </div>
  )
}

function PermissionAllowedActions({ parsed }: DetailsProps) {
  const t = useTranslations("Folder.chat.permissionDialog")
  if (!parsed.allowedPrompts.length) return null
  return (
    <div className="space-y-2 text-xs leading-5">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Terminal className="size-3.5" />
        <span>{t("allowedActions")}</span>
      </div>
      {parsed.allowedPrompts.map((item, index) => (
        <div key={index} className="flex flex-wrap items-start gap-2">
          {item.tool && (
            <Badge
              variant="outline"
              className="max-w-full rounded-md text-[10px] whitespace-normal"
            >
              {item.tool}
            </Badge>
          )}
          <span className="min-w-0 flex-1">{item.prompt}</span>
        </div>
      ))}
    </div>
  )
}

function PermissionWeb({ parsed }: DetailsProps) {
  if (!parsed.url && !parsed.query) return null
  return (
    <div className="space-y-2 text-xs leading-5">
      {parsed.url && (
        <div className="flex items-start gap-2">
          <Globe className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 font-mono">{parsed.url}</span>
        </div>
      )}
      {parsed.query && (
        <div className="flex items-start gap-2">
          <Search className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0">{parsed.query}</span>
        </div>
      )}
      {parsed.prompt && <MessageResponse>{parsed.prompt}</MessageResponse>}
    </div>
  )
}

function PermissionFallback({ parsed }: DetailsProps) {
  const structured =
    parsed.command ||
    parsed.fileChanges.length ||
    parsed.planEntries.length ||
    parsed.planExplanation ||
    parsed.planMarkdown ||
    parsed.allowedPrompts.length ||
    parsed.modeTarget ||
    parsed.url ||
    parsed.query
  if (structured) return null
  return parsed.contentText ? (
    <div className="text-xs leading-5">
      <MessageResponse>{parsed.contentText}</MessageResponse>
    </div>
  ) : (
    <pre className="text-xs leading-5 whitespace-pre-wrap [overflow-wrap:anywhere]">
      {parsed.jsonPreview}
    </pre>
  )
}
