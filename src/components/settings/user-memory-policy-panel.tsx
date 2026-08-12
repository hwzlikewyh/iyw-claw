"use client"

import { FileStack, Settings2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Switch } from "@/components/ui/switch"
import {
  USER_MEMORY_DOCUMENTS,
  type UserMemoryDocumentId,
  type UserMemoryDraft,
} from "@/lib/user-memory-documents"

interface UserMemoryPolicyPanelProps {
  draft: UserMemoryDraft
  disabled: boolean
  onChange: (next: UserMemoryDraft) => void
}

interface ToggleRowProps {
  id: string
  label: string
  description: string
  checked: boolean
  disabled: boolean
  onCheckedChange: (checked: boolean) => void
}

function ToggleRow({
  id,
  label,
  description,
  checked,
  disabled,
  onCheckedChange,
}: ToggleRowProps) {
  return (
    <div className="flex items-center justify-between gap-3 py-2">
      <div className="min-w-0 space-y-0.5">
        <label htmlFor={id} className="text-sm font-medium">
          {label}
        </label>
        <p className="text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
      <Switch
        id={id}
        aria-label={label}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
      />
    </div>
  )
}

export function UserMemoryPolicyPanel({
  draft,
  disabled,
  onChange,
}: UserMemoryPolicyPanelProps) {
  const t = useTranslations("UserMemorySettings")
  const childDisabled = disabled || !draft.enabled

  const updateDocument = (
    documentId: UserMemoryDocumentId,
    enabled: boolean
  ) => {
    onChange({
      ...draft,
      documents: {
        ...draft.documents,
        [documentId]: {
          ...draft.documents[documentId],
          enabled,
        },
      },
    })
  }

  return (
    <section className="border-y bg-muted/20 px-3 py-3">
      <div className="mb-2 flex items-center gap-2">
        <Settings2 className="h-4 w-4 text-muted-foreground" aria-hidden />
        <div>
          <h2 className="text-sm font-semibold">{t("policy.title")}</h2>
          <p className="text-xs text-muted-foreground">
            {t("policy.description")}
          </p>
        </div>
      </div>

      <ToggleRow
        id="user-memory-enabled"
        label={t("policy.enabled")}
        description={t("policy.enabledDescription")}
        checked={draft.enabled}
        disabled={disabled}
        onCheckedChange={(enabled) => onChange({ ...draft, enabled })}
      />

      <div className="grid border-t md:grid-cols-2 md:gap-x-6">
        <ToggleRow
          id="user-memory-agent-write"
          label={t("policy.agentWriteEnabled")}
          description={t("policy.agentWriteDescription")}
          checked={draft.agentWriteEnabled}
          disabled={childDisabled}
          onCheckedChange={(agentWriteEnabled) =>
            onChange({ ...draft, agentWriteEnabled })
          }
        />
        <ToggleRow
          id="user-memory-subagent-inheritance"
          label={t("policy.inheritToSubagents")}
          description={t("policy.inheritDescription")}
          checked={draft.inheritToSubagents}
          disabled={childDisabled}
          onCheckedChange={(inheritToSubagents) =>
            onChange({ ...draft, inheritToSubagents })
          }
        />
      </div>

      <div className="border-t pt-3">
        <div className="mb-2 flex items-center gap-2 text-xs font-medium">
          <FileStack
            className="h-3.5 w-3.5 text-muted-foreground"
            aria-hidden
          />
          {t("policy.documentsTitle")}
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {USER_MEMORY_DOCUMENTS.map((document) => {
            const label = t(document.labelKey)
            return (
              <div
                key={document.id}
                className="flex min-w-0 items-center justify-between gap-2 rounded-md border bg-background/50 px-3 py-2"
              >
                <span className="truncate text-xs font-medium">{label}</span>
                <Switch
                  aria-label={t("policy.documentToggle", { document: label })}
                  checked={draft.documents[document.id].enabled}
                  disabled={childDisabled}
                  onCheckedChange={(enabled) =>
                    updateDocument(document.id, enabled)
                  }
                />
              </div>
            )
          })}
        </div>
      </div>
    </section>
  )
}
