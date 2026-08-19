"use client"

import { Settings2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Switch } from "@/components/ui/switch"
import { SettingRow, SettingSection } from "@/components/settings/settings-ui"
import type { UserMemoryDraft } from "@/lib/user-memory-documents"

interface UserMemoryPolicyPanelProps {
  draft: UserMemoryDraft
  disabled: boolean
  onChange: (next: UserMemoryDraft) => void
}

export function UserMemoryPolicyPanel({
  draft,
  disabled,
  onChange,
}: UserMemoryPolicyPanelProps) {
  const t = useTranslations("UserMemorySettings")
  const childDisabled = disabled || !draft.enabled

  return (
    <SettingSection
      icon={Settings2}
      title={t("policy.title")}
      description={t("policy.description")}
    >
      <SettingRow
        title={t("policy.enabled")}
        description={t("policy.enabledDescription")}
      >
        <Switch
          id="user-memory-enabled"
          aria-label={t("policy.enabled")}
          checked={draft.enabled}
          disabled={disabled}
          onCheckedChange={(enabled) => onChange({ ...draft, enabled })}
        />
      </SettingRow>

      <SettingRow
        title={t("policy.agentWriteEnabled")}
        description={t("policy.agentWriteDescription")}
      >
        <Switch
          id="user-memory-agent-write"
          aria-label={t("policy.agentWriteEnabled")}
          checked={draft.agentWriteEnabled}
          disabled={childDisabled}
          onCheckedChange={(agentWriteEnabled) =>
            onChange({ ...draft, agentWriteEnabled })
          }
        />
      </SettingRow>

      <SettingRow
        title={t("policy.inheritToSubagents")}
        description={t("policy.inheritDescription")}
      >
        <Switch
          id="user-memory-subagent-inheritance"
          aria-label={t("policy.inheritToSubagents")}
          checked={draft.inheritToSubagents}
          disabled={childDisabled}
          onCheckedChange={(inheritToSubagents) =>
            onChange({ ...draft, inheritToSubagents })
          }
        />
      </SettingRow>
    </SettingSection>
  )
}
