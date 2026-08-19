"use client"

import { useTranslations } from "next-intl"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { orderSessionSelectors } from "@/lib/session-selector-order"
import { ModelIcon } from "@/components/chat/model-icon"
import type { AgentOptionsSnapshot, SessionConfigOptionInfo } from "@/lib/types"

const DEFAULT_SENTINEL = "__iyw_claw_default__"

interface SnapshotEditorProps {
  snapshot: AgentOptionsSnapshot
  defaultModeId: string | null
  overrideModeId: string | null
  overrideConfigValues: Record<string, string>
  onModeChange: (modeId: string | null) => void
  onConfigChange: (optionId: string, valueId: string | null) => void
  disabled?: boolean
}

export function SnapshotEditor({
  snapshot,
  defaultModeId,
  overrideModeId,
  overrideConfigValues,
  onModeChange,
  onConfigChange,
  disabled,
}: SnapshotEditorProps) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  const hasModes = Boolean(snapshot.modes?.available_modes.length)
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
              defaultModeId={defaultModeId}
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
  defaultModeId: string | null
  overrideModeId: string | null
  onChange: (modeId: string | null) => void
  disabled?: boolean
}

function ModeRow({
  modes,
  agentDefaultModeId,
  defaultModeId,
  overrideModeId,
  onChange,
  disabled,
}: ModeRowProps) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  const agentDefaultName =
    modes.find((mode) => mode.id === agentDefaultModeId)?.name ??
    agentDefaultModeId
  const effectiveDefaultModeId = defaultModeId ?? agentDefaultModeId
  const defaultModeName =
    modes.find((mode) => mode.id === effectiveDefaultModeId)?.name ??
    agentDefaultName
  const selectedModeId =
    !overrideModeId || overrideModeId === effectiveDefaultModeId
      ? DEFAULT_SENTINEL
      : overrideModeId
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 space-y-0.5">
        <label className="text-sm font-medium">{t("modeLabel")}</label>
        <p className="text-xs text-muted-foreground">
          {t("agentDefaultHint", { value: agentDefaultName })}
        </p>
      </div>
      <Select
        value={selectedModeId}
        onValueChange={(value) =>
          onChange(value === DEFAULT_SENTINEL ? null : value)
        }
        disabled={disabled}
      >
        <SelectTrigger size="sm" className="w-44">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={DEFAULT_SENTINEL}>{defaultModeName}</SelectItem>
          {modes
            .filter((mode) => mode.id !== effectiveDefaultModeId)
            .map((mode) => (
              <SelectItem key={mode.id} value={mode.id}>
                {mode.name}
              </SelectItem>
            ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function ConfigOptionRow({
  option,
  overrideValue,
  onChange,
  disabled,
}: {
  option: SessionConfigOptionInfo
  overrideValue: string | null
  onChange: (valueId: string | null) => void
  disabled?: boolean
}) {
  const t = useTranslations("AcpAgentSettings.multiAgent")
  if (option.kind.type !== "select") return null

  const allOptions = option.kind.groups.length
    ? option.kind.groups.flatMap((group) => group.options)
    : option.kind.options
  const agentDefault = option.kind.current_value
  const agentDefaultLabel =
    allOptions.find((item) => item.value === agentDefault)?.name ?? agentDefault

  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 space-y-0.5">
        <label className="text-sm font-medium">{option.name}</label>
        <p className="text-xs text-muted-foreground">
          {t("agentDefaultHint", { value: agentDefaultLabel })}
        </p>
      </div>
      <Select
        value={overrideValue ?? DEFAULT_SENTINEL}
        onValueChange={(value) =>
          onChange(value === DEFAULT_SENTINEL ? null : value)
        }
        disabled={disabled}
      >
        <SelectTrigger size="sm" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={DEFAULT_SENTINEL}>
            {t("defaultOptionLabel", { value: agentDefaultLabel })}
          </SelectItem>
          {option.kind.groups.length
            ? option.kind.groups.map((group) => (
                <SelectGroup key={group.group}>
                  <SelectLabel>{group.name}</SelectLabel>
                  {group.options.map((item) => (
                    <SelectItem
                      key={`${group.group}-${item.value}`}
                      value={item.value}
                    >
                      {option.id === "model" ? (
                        <ModelIcon src={item.iconUrl} />
                      ) : null}
                      {item.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              ))
            : option.kind.options.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {option.id === "model" ? (
                    <ModelIcon src={item.iconUrl} />
                  ) : null}
                  {item.name}
                </SelectItem>
              ))}
        </SelectContent>
      </Select>
    </div>
  )
}
