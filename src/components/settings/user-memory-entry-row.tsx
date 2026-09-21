"use client"

import { useState } from "react"
import { Check, ChevronDown, Pause, Pencil, RotateCcw, X } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import type { UserMemoryEntry } from "@/lib/user-memory-entries"
import type { ForgetUserMemoryResult } from "@/lib/user-memory-entries"
import { MemoryHistoryButton } from "./user-memory-history"
import { UserMemoryForgetDialog } from "./user-memory-forget-dialog"

interface EntryRowProps {
  entry: UserMemoryEntry
  disabled: boolean
  onSave: (content: string) => Promise<boolean>
  onToggle: () => Promise<boolean>
  onForget: (purgeBackups: boolean) => Promise<ForgetUserMemoryResult | null>
}

export function UserMemoryEntryRow(props: EntryRowProps) {
  const t = useTranslations("UserMemorySettings.entries")
  const [editing, setEditing] = useState(false)
  const [content, setContent] = useState(props.entry.content)
  const save = async () => {
    if (await props.onSave(content)) setEditing(false)
  }
  return (
    <li className="space-y-2 border-b py-3 last:border-b-0">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          {editing ? (
            <Textarea
              aria-label={t("edit")}
              value={content}
              onChange={(event) => setContent(event.target.value)}
              disabled={props.disabled}
              className="min-h-24 text-sm"
            />
          ) : (
            <p className="whitespace-pre-wrap break-words text-sm leading-6">
              {props.entry.content}
            </p>
          )}
        </div>
        {editing ? (
          <EditActions
            disabled={props.disabled}
            canSave={!!content.trim() && content !== props.entry.content}
            onSave={() => void save()}
            onCancel={() => setEditing(false)}
          />
        ) : (
          <EntryActions
            {...props}
            onEdit={() => {
              setContent(props.entry.content)
              setEditing(true)
            }}
          />
        )}
      </div>
      <EntryMetadata entry={props.entry} />
    </li>
  )
}

function EditActions({
  disabled,
  canSave,
  onSave,
  onCancel,
}: {
  disabled: boolean
  canSave: boolean
  onSave: () => void
  onCancel: () => void
}) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <div className="flex shrink-0 gap-1">
      <Button
        variant="ghost"
        size="icon"
        title={t("save")}
        aria-label={t("save")}
        disabled={disabled || !canSave}
        onClick={onSave}
      >
        <Check className="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        title={t("cancel")}
        aria-label={t("cancel")}
        disabled={disabled}
        onClick={onCancel}
      >
        <X className="size-4" />
      </Button>
    </div>
  )
}

function EntryActions({
  entry,
  disabled,
  onEdit,
  onToggle,
  onForget,
}: EntryRowProps & { onEdit: () => void }) {
  const t = useTranslations("UserMemorySettings.entries")
  const label = t(entry.active ? "disable" : "restore")
  return (
    <div className="flex shrink-0 gap-1">
      <MemoryHistoryButton id={entry.id} />
      <Button
        variant="ghost"
        size="icon"
        title={t("edit")}
        aria-label={t("edit")}
        disabled={disabled || !entry.active}
        onClick={onEdit}
      >
        <Pencil className="size-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        title={label}
        aria-label={label}
        disabled={disabled}
        onClick={() => void onToggle()}
      >
        {entry.active ? (
          <Pause className="size-4" />
        ) : (
          <RotateCcw className="size-4" />
        )}
      </Button>
      <UserMemoryForgetDialog disabled={disabled} onForget={onForget} />
    </div>
  )
}

function EntryMetadata({ entry }: { entry: UserMemoryEntry }) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <div className="space-y-1 text-xs text-muted-foreground">
      <div className="flex flex-wrap gap-x-3 gap-y-1">
        {entry.generated && <span>{t("generated")}</span>}
        <span
          className={
            entry.active
              ? "text-emerald-700 dark:text-emerald-400"
              : "text-amber-700 dark:text-amber-400"
          }
        >
          {t(entry.active ? "active" : "inactive")}
        </span>
        <span>
          {t(entry.scopeType === "global" ? "globalScope" : "workspaceScope")}
        </span>
      </div>
      {entry.sources.length > 0 && (
        <details>
          <summary className="flex w-fit cursor-pointer items-center gap-1 py-1">
            <ChevronDown className="size-3" />
            {t("sources")}
          </summary>
          <ul className="space-y-1 break-all font-mono">
            {entry.sources.map((source) => (
              <li key={source}>{source}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )
}
