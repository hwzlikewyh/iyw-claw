import { useCallback, useMemo, useState } from "react"
import {
  isFastConfigOption,
  isReasoningConfigOption,
} from "@/lib/model-config-groups"
import type {
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
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
      const configured = behavior.kind.current_value
      const current = metadata.reasoningOptions.some(
        (item) => item.value === configured
      )
        ? configured
        : metadata.reasoningOptions.some(
              (item) => item.value === metadata.defaultReasoningEffort
            )
          ? metadata.defaultReasoningEffort!
          : metadata.reasoningOptions[0].value
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
      return [{ ...behavior, kind: { ...behavior.kind, current_value: current } }]
    }
    return [behavior]
  })
}

export function useModelBehaviorPreview(
  models: SessionConfigSelectOptionInfo[],
  currentValue: string,
  behaviorOptions: SessionConfigOptionInfo[]
) {
  const [behaviorModelValue, setBehaviorModelValue] = useState<string | null>(
    null
  )
  const behaviorOptionsForModel = useCallback(
    (model: SessionConfigSelectOptionInfo) =>
      optionsForModel(model, currentValue, behaviorOptions),
    [behaviorOptions, currentValue]
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
  }
}
