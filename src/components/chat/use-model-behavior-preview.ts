import { useCallback, useMemo, useState } from "react"
import {
  isFastConfigOption,
  isReasoningConfigOption,
} from "@/lib/model-config-groups"
import {
  getCachedGatewayModels,
  resolveModelReasoningEffort,
} from "@/lib/gateway-model-catalog"
import {
  buildContextWindowOption,
  CONTEXT_WINDOW_CONFIG_ID,
} from "@/lib/model-context-window"
import { getSavedModelConfigPreferences } from "@/lib/selector-prefs-storage"
import type {
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
  AgentType,
} from "@/lib/types"

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
  const [behaviorModelValue, setBehaviorModelValue] = useState<string | null>(
    null
  )
  const behaviorOptionsForModel = useCallback(
    (model: SessionConfigSelectOptionInfo) => {
      const context = agentType
        ? buildContextWindowOption({
            agentType,
            model: getCachedGatewayModels(agentType ?? undefined).find(
              (item) => item.id === model.value
            ),
            configured:
              model.value === currentValue
                ? behaviorOptions.find(
                    (option) => option.id === CONTEXT_WINDOW_CONFIG_ID
                  )?.kind.current_value
                : agentType
                  ? getSavedModelConfigPreferences(agentType)[model.value]?.[
                      CONTEXT_WINDOW_CONFIG_ID
                    ]
                  : undefined,
          })
        : model.value === currentValue
          ? behaviorOptions.find(
              (option) => option.id === CONTEXT_WINDOW_CONFIG_ID
            )
          : null
      const options = optionsForModel(
        model,
        currentValue,
        behaviorOptions
      ).filter((option) => option.id !== CONTEXT_WINDOW_CONFIG_ID)
      return context ? [...options, context] : options
    },
    [agentType, behaviorOptions, currentValue]
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
