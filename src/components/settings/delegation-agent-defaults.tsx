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

import { useCallback, useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  AGENT_LABELS,
  type AgentDelegationDefaults,
  type AgentOptionsSnapshot,
  type AgentType,
  type SessionConfigOptionInfo,
} from "@/lib/types"
import {
  getFixedAgentOptions,
  loadFixedAgentOptions,
} from "@/lib/fixed-agent-options"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"
import { orderSessionSelectors } from "@/lib/session-selector-order"

// Sentinel `value` slot used by the top "Default" Select item in mode +
// config-option rows. Picking it clears the override (sets it back to
// `null`) so the agent's own default takes effect at runtime. Must not
// collide with any real option id any agent could emit — the iyw-claw
// prefix makes a collision implausible.
const DEFAULT_SENTINEL = "__iyw_claw_default__"

const AGENT_TYPES: AgentType[] = [
  "claude_code",
  "codex",
  "open_code",
  "gemini",
  "open_claw",
  "cline",
  "hermes",
  "code_buddy",
  "kimi_code",
  "pi",
  "grok",
]

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
  const tSessionConfig = useTranslations("Folder.chat.messageInput")
  const translator = tSessionConfig as unknown as SessionConfigTranslator
  const [selectedAgent, setSelectedAgent] = useState<AgentType>("claude_code")
  const [catalogVersion, setCatalogVersion] = useState(0)
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
  const fixedSnapshot = getFixedAgentOptions(selectedAgent)
  const snapshot = {
    ...fixedSnapshot,
    config_options: fixedSnapshot.config_options.map((option) =>
      localizeSessionConfigOption(option, translator)
    ),
  }

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

  const current = value[selectedAgent] ?? null
  const currentModeId = current?.mode_id ?? null
  const currentConfigValues = current?.config_values ?? {}

  const setMode = (modeId: string | null) => {
    const next: AgentDelegationDefaults = {
      mode_id: modeId ?? undefined,
      config_values: { ...currentConfigValues },
    }
    updateAgentDefaults(selectedAgent, next)
  }

  const setConfigValue = (optionId: string, valueId: string | null) => {
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

      <div
        role="tablist"
        aria-label={t("tabAgentDefaults")}
        className="flex flex-wrap gap-1 rounded-2xl bg-muted p-1"
      >
        {AGENT_TYPES.map((agent) => (
          <button
            key={agent}
            type="button"
            role="tab"
            aria-selected={selectedAgent === agent}
            disabled={disabled}
            onClick={() => setSelectedAgent(agent)}
            className={
              "rounded-xl px-3 py-1 text-xs font-medium transition-colors disabled:opacity-50 " +
              (selectedAgent === agent
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground")
            }
          >
            {AGENT_LABELS[agent]}
          </button>
        ))}
      </div>

      <div className="min-h-[120px] rounded-lg border bg-card/50 p-3">
        <SnapshotEditor
          snapshot={snapshot}
          overrideModeId={currentModeId}
          overrideConfigValues={currentConfigValues}
          onModeChange={setMode}
          onConfigChange={setConfigValue}
          disabled={disabled}
        />
      </div>
    </div>
  )
}

interface SnapshotEditorProps {
  snapshot: AgentOptionsSnapshot
  overrideModeId: string | null
  overrideConfigValues: Record<string, string>
  onModeChange: (modeId: string | null) => void
  onConfigChange: (optionId: string, valueId: string | null) => void
  disabled?: boolean
}

function SnapshotEditor({
  snapshot,
  overrideModeId,
  overrideConfigValues,
  onModeChange,
  onConfigChange,
  disabled,
}: SnapshotEditorProps) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  const hasModes =
    snapshot.modes !== null &&
    snapshot.modes !== undefined &&
    snapshot.modes.available_modes.length > 0
  const hasOptions = snapshot.config_options.length > 0

  if (!hasModes && !hasOptions) {
    return (
      <p className="text-xs text-muted-foreground">{t("noConfigAvailable")}</p>
    )
  }

  const selectors = orderSessionSelectors(hasModes, snapshot.config_options)
  return (
    <div className="space-y-4">
      {selectors.map((selector) => {
        if (selector.kind === "mode") {
          if (!snapshot.modes) return null
          return (
            <ModeRow
              key="__mode__"
              modes={snapshot.modes.available_modes}
              agentDefaultModeId={snapshot.modes.current_mode_id}
              overrideModeId={overrideModeId}
              onChange={onModeChange}
              disabled={disabled}
            />
          )
        }
        const option = selector.option
        return (
          <ConfigOptionRow
            key={`config:${option.id}`}
            option={option}
            overrideValue={overrideConfigValues[option.id] ?? null}
            onChange={(valueId) => onConfigChange(option.id, valueId)}
            disabled={disabled}
          />
        )
      })}
    </div>
  )
}

interface ModeRowProps {
  modes: Array<{ id: string; name: string; description?: string | null }>
  agentDefaultModeId: string
  overrideModeId: string | null
  onChange: (modeId: string | null) => void
  disabled?: boolean
}

function ModeRow({
  modes,
  agentDefaultModeId,
  overrideModeId,
  onChange,
  disabled,
}: ModeRowProps) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  const agentDefaultName =
    modes.find((m) => m.id === agentDefaultModeId)?.name ?? agentDefaultModeId
  // When no override exists, show the Default sentinel so the user can
  // see "no override is set" at a glance; selecting any real mode below
  // applies an override, selecting the sentinel clears it.
  const selectValue = overrideModeId ?? DEFAULT_SENTINEL
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="space-y-0.5 min-w-0">
        <label className="text-sm font-medium">{t("modeLabel")}</label>
        <p className="text-xs text-muted-foreground">
          {t("agentDefaultHint", { value: agentDefaultName })}
        </p>
      </div>
      <Select
        value={selectValue}
        onValueChange={(v) => onChange(v === DEFAULT_SENTINEL ? null : v)}
        disabled={disabled}
      >
        <SelectTrigger size="sm" className="w-44">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={DEFAULT_SENTINEL}>
            {t("defaultOptionLabel", { value: agentDefaultName })}
          </SelectItem>
          {modes.map((mode) => (
            <SelectItem key={mode.id} value={mode.id}>
              {mode.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

interface ConfigOptionRowProps {
  option: SessionConfigOptionInfo
  overrideValue: string | null
  onChange: (valueId: string | null) => void
  disabled?: boolean
}

function ConfigOptionRow({
  option,
  overrideValue,
  onChange,
  disabled,
}: ConfigOptionRowProps) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  if (option.kind.type !== "select") return null

  const allOptions =
    option.kind.groups.length > 0
      ? option.kind.groups.flatMap((g) => g.options)
      : option.kind.options
  const agentDefault = option.kind.current_value
  const agentDefaultLabel =
    allOptions.find((o) => o.value === agentDefault)?.name ?? agentDefault
  // When no override exists, the trigger shows the Default sentinel item
  // so the user can tell "I'm inheriting" apart from "I picked the
  // agent's current default explicitly" — the latter would stick to that
  // literal value even if the agent later changes its own default.
  const selectValue = overrideValue ?? DEFAULT_SENTINEL

  return (
    <div className="flex items-start justify-between gap-3">
      <div className="space-y-0.5 min-w-0">
        <label className="text-sm font-medium">{option.name}</label>
        <p className="text-xs text-muted-foreground">
          {t("agentDefaultHint", { value: agentDefaultLabel })}
        </p>
      </div>
      <Select
        value={selectValue}
        onValueChange={(v) => onChange(v === DEFAULT_SENTINEL ? null : v)}
        disabled={disabled}
      >
        <SelectTrigger size="sm" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={DEFAULT_SENTINEL}>
            {t("defaultOptionLabel", { value: agentDefaultLabel })}
          </SelectItem>
          {option.kind.groups.length > 0
            ? option.kind.groups.map((group) => (
                <SelectGroup key={group.group}>
                  <SelectLabel>{group.name}</SelectLabel>
                  {group.options.map((item) => (
                    <SelectItem
                      key={`${group.group}-${item.value}`}
                      value={item.value}
                    >
                      {item.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              ))
            : option.kind.options.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.name}
                </SelectItem>
              ))}
        </SelectContent>
      </Select>
    </div>
  )
}
