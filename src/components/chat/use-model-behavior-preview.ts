import { useCallback, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import {
  isFastConfigOption,
  isModelBehaviorConfigOption,
  isReasoningConfigOption,
} from "@/lib/model-config-groups"
import {
  buildAgentOptionsSnapshot,
  getCachedGatewayModels,
  resolveModelReasoningEffort,
} from "@/lib/gateway-model-catalog"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"
import {
  COMPACTION_THRESHOLD_CONFIG_ID,
  CONTEXT_WINDOW_CONFIG_ID,
} from "@/lib/model-context-window"

import { getSavedModelConfigPreferences } from "@/lib/selector-prefs-storage"
import type {
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
  AgentType,
} from "@/lib/types"

function behaviorOrder(option: SessionConfigOptionInfo): number {
  if (option.id === COMPACTION_THRESHOLD_CONFIG_ID) return 2
  return option.id === CONTEXT_WINDOW_CONFIG_ID ? 1 : 0
}

function optionsForModel(
  model: SessionConfigSelectOptionInfo,
  currentValue: string,
  behaviorOptions: SessionConfigOptionInfo[]
): SessionConfigOptionInfo[] {
  const metadata = model.modelBehavior
  if (!metadata) return model.value === currentValue ? behaviorOptions : []
  return behaviorOptions.flatMap((behavior) => {
    if (isReasoningConfigOption(behavior)) {
      if (metadata.reasoningOptions.length === 0) return []
      const configured =
        metadata.savedReasoningEffort ??
        (model.value === currentValue ? behavior.kind.current_value : undefined)
      const current = resolveModelReasoningEffort(model, configured)
      if (!current) return []
      return [
        {
          ...behavior,
          kind: {
            ...behavior.kind,
            current_value: current,
            options: metadata.reasoningOptions,
          },
        },
      ]
    }
    if (isFastConfigOption(behavior)) {
      if (!metadata.fastModeSupported) return []
      const current =
        behavior.kind.current_value === "on" ||
        behavior.kind.current_value === "off"
          ? behavior.kind.current_value
          : metadata.fastModeDefaultEnabled
            ? "on"
            : "off"
      return [
        { ...behavior, kind: { ...behavior.kind, current_value: current } },
      ]
    }
    return [behavior]
  })
}

export function useModelBehaviorPreview(
  models: SessionConfigSelectOptionInfo[],
  currentValue: string,
  behaviorOptions: SessionConfigOptionInfo[],
  agentType?: AgentType | null
) {
  const t = useTranslations("Folder.chat.messageInput")
  const [behaviorModelValue, setBehaviorModelValue] = useState<string | null>(
    null
  )
  const behaviorOptionsForModel = useCallback(
    (model: SessionConfigSelectOptionInfo) => {
      const catalogModel = getCachedGatewayModels(agentType ?? undefined).find(
        (item) => item.id === model.value
      )
      if (!agentType || !catalogModel)
        return optionsForModel(model, currentValue, behaviorOptions)
      const selectedValues =
        model.value === currentValue
          ? Object.fromEntries(
              behaviorOptions.map((option) => [
                option.id,
                option.kind.current_value,
              ])
            )
          : {}
      const saved = getSavedModelConfigPreferences(agentType)
      const snapshot = buildAgentOptionsSnapshot(
        agentType,
        [catalogModel],
        { ...selectedValues, model: model.value },
        {
          ...saved,
          [model.value]: { ...saved[model.value], ...selectedValues },
        }
      )
      return snapshot.config_options
        .filter(isModelBehaviorConfigOption)
        .sort((left, right) => behaviorOrder(left) - behaviorOrder(right))
        .map((option) =>
          localizeSessionConfigOption(option, t as SessionConfigTranslator)
        )
    },
    [agentType, behaviorOptions, currentValue, t]
  )
  const activeModel = useMemo(
    () => models.find((model) => model.value === behaviorModelValue) ?? null,
    [behaviorModelValue, models]
  )
  const activeBehaviorOptions = useMemo(
    () => (activeModel ? behaviorOptionsForModel(activeModel) : []),
    [activeModel, behaviorOptionsForModel]
  )
  const behaviorSummary = useMemo(
    () =>
      behaviorOptions
        .map((option) => {
          const current = option.kind.options.find(
            ({ value }) => value === option.kind.current_value
          )
          return `${option.name}：${current?.name ?? option.kind.current_value}`
        })
        .join(" · "),
    [behaviorOptions]
  )
  return {
    behaviorModelValue,
    setBehaviorModelValue,
    behaviorOptionsForModel,
    activeBehaviorOptions,
    behaviorSummary,
    activeModel,
  }
}
