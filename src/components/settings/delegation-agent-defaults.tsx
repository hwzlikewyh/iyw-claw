"use client"

/**
 * Per-agent defaults editor for delegation. Lives inside the
 * "Multi-Agent Collaboration" settings card under the "Agent defaults" tab.
 *
 * Isolation guarantees (critical — see the v2 plan):
 *   1. Options come from the product-owned fixed catalog. Opening this panel
 *      never launches an Agent process.
 *   2. Saving a value here does NOT call `acpSetConfigOption` or write to
 *      `selector-prefs-storage.ts` localStorage. The chat input's own
 *      selectors are untouched. Persistence happens through the parent's
 *      `setDelegationSettings` save action only.
 */

import { useCallback, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { SnapshotEditor } from "@/components/settings/delegation-agent-defaults-editor"
import {
  AGENT_LABELS,
  type AgentDelegationDefaults,
  type AgentType,
} from "@/lib/types"
import {
  getFixedAgentOptions,
  loadFixedAgentOptions,
} from "@/lib/fixed-agent-options"
import { automaticAgentMode } from "@/lib/automatic-agent-mode"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"
import { useAcpAgents } from "@/hooks/use-acp-agents"

export interface DelegationAgentDefaultsPanelProps {
  value: Partial<Record<AgentType, AgentDelegationDefaults>>
  onChange: (next: Partial<Record<AgentType, AgentDelegationDefaults>>) => void
  disabled?: boolean
}

export function DelegationAgentDefaultsPanel({
  value,
  onChange,
  disabled,
}: DelegationAgentDefaultsPanelProps) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  const { status: accountStatus } = useIywAccount()
  const { agents, fresh } = useAcpAgents()
  const tSessionConfig = useTranslations("Folder.chat.messageInput")
  const translator = tSessionConfig as unknown as SessionConfigTranslator
  const availableAgents = useMemo(
    () => agents.filter((agent) => agent.enabled && agent.installed_version),
    [agents]
  )
  const [requestedAgent, setRequestedAgent] = useState<AgentType | null>(null)
  const [catalogVersion, setCatalogVersion] = useState(0)
  const selectedAgent =
    requestedAgent &&
    availableAgents.some((agent) => agent.agent_type === requestedAgent)
      ? requestedAgent
      : (availableAgents[0]?.agent_type ?? null)
  useEffect(() => {
    if (accountStatus !== "authenticated") return
    let active = true
    void loadFixedAgentOptions().then(() => {
      if (active) setCatalogVersion((version) => version + 1)
    })
    return () => {
      active = false
    }
  }, [accountStatus])
  void catalogVersion
  const fixedSnapshot = selectedAgent
    ? getFixedAgentOptions(selectedAgent)
    : null
  const snapshot = fixedSnapshot
    ? {
        ...fixedSnapshot,
        config_options: fixedSnapshot.config_options.map((option) =>
          localizeSessionConfigOption(option, translator)
        ),
      }
    : null

  const updateAgentDefaults = useCallback(
    (agent: AgentType, next: AgentDelegationDefaults | null) => {
      const updated: Partial<Record<AgentType, AgentDelegationDefaults>> = {
        ...value,
      }
      if (
        next === null ||
        ((!next.mode_id || next.mode_id.length === 0) &&
          Object.keys(next.config_values).length === 0)
      ) {
        delete updated[agent]
      } else {
        updated[agent] = next
      }
      onChange(updated)
    },
    [value, onChange]
  )

  const current = selectedAgent ? (value[selectedAgent] ?? null) : null
  const currentModeId = current?.mode_id ?? null
  const currentConfigValues = current?.config_values ?? {}
  const automaticModeId = selectedAgent
    ? (automaticAgentMode(selectedAgent)?.id ?? null)
    : null

  const setMode = (modeId: string | null) => {
    if (!selectedAgent) return
    const next: AgentDelegationDefaults = {
      mode_id: modeId ?? undefined,
      config_values: { ...currentConfigValues },
    }
    updateAgentDefaults(selectedAgent, next)
  }

  const setConfigValue = (optionId: string, valueId: string | null) => {
    if (!selectedAgent) return
    const nextConfig = { ...currentConfigValues }
    if (valueId === null) {
      delete nextConfig[optionId]
    } else {
      nextConfig[optionId] = valueId
    }
    const next: AgentDelegationDefaults = {
      mode_id: currentModeId ?? undefined,
      config_values: nextConfig,
    }
    updateAgentDefaults(selectedAgent, next)
  }

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground leading-5">
        {t("agentDefaultsDescription")}
      </p>

      {!fresh ? (
        <div className="border-y py-6 text-center text-xs text-muted-foreground">
          {t("probing")}
        </div>
      ) : null}

      {fresh && !availableAgents.length ? (
        <div className="border-y py-6 text-center text-xs text-muted-foreground">
          {t("noInstalledAgents")}
        </div>
      ) : null}

      {availableAgents.length ? (
        <div
          role="tablist"
          aria-label={t("tabAgentDefaults")}
          className="flex flex-wrap gap-1 border-b pb-2"
        >
          {availableAgents.map((agent) => (
            <button
              key={agent.agent_type}
              type="button"
              role="tab"
              aria-selected={selectedAgent === agent.agent_type}
              disabled={disabled}
              onClick={() => setRequestedAgent(agent.agent_type)}
              className={
                "border-b-2 border-transparent px-3 py-1 text-xs font-medium transition-colors disabled:opacity-50 " +
                (selectedAgent === agent.agent_type
                  ? "border-primary text-foreground"
                  : "text-muted-foreground hover:text-foreground")
              }
            >
              {AGENT_LABELS[agent.agent_type]}
            </button>
          ))}
        </div>
      ) : null}

      {snapshot && selectedAgent ? (
        <div className="min-h-[120px] border bg-card/50 p-3">
          <SnapshotEditor
            snapshot={snapshot}
            defaultModeId={automaticModeId}
            overrideModeId={currentModeId}
            overrideConfigValues={currentConfigValues}
            onModeChange={setMode}
            onConfigChange={setConfigValue}
            disabled={disabled}
          />
        </div>
      ) : null}
    </div>
  )
}
