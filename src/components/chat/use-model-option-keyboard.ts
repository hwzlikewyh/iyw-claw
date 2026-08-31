import { useCallback } from "react"
import type { ModelOptionRow } from "@/lib/model-config-groups"
import type {
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"

interface ModelOptionKeyboardParams {
  rows: ModelOptionRow[]
  optionRowIndices: number[]
  activeIndexClamped: number
  optionCount: number
  currentValue: string
  moveActiveTo: (next: number) => void
  onSelect: (value: string) => void
  behaviorOptionsForModel: (
    model: SessionConfigSelectOptionInfo
  ) => SessionConfigOptionInfo[]
  showBehavior: (modelValue: string) => void
}

export function useModelOptionKeyboard({
  rows,
  optionRowIndices,
  activeIndexClamped,
  optionCount,
  currentValue,
  moveActiveTo,
  onSelect,
  behaviorOptionsForModel,
  showBehavior,
}: ModelOptionKeyboardParams) {
  return useCallback(
    (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (event.nativeEvent.isComposing || event.key === "Process") return
      switch (event.key) {
        case "ArrowDown":
          event.preventDefault()
          moveActiveTo(activeIndexClamped + 1)
          break
        case "ArrowUp":
          event.preventDefault()
          moveActiveTo(activeIndexClamped - 1)
          break
        case "Home":
          event.preventDefault()
          moveActiveTo(0)
          break
        case "End":
          event.preventDefault()
          moveActiveTo(optionCount - 1)
          break
        case "Enter": {
          const rowIndex = optionRowIndices[activeIndexClamped]
          const row = rowIndex != null ? rows[rowIndex] : undefined
          if (!row || row.kind !== "option") break
          event.preventDefault()
          if (
            row.option.value === currentValue &&
            behaviorOptionsForModel(row.option).length > 0
          ) {
            showBehavior(row.option.value)
          } else {
            onSelect(row.option.value)
          }
          break
        }
        default:
          break
      }
    },
    [
      activeIndexClamped,
      behaviorOptionsForModel,
      currentValue,
      moveActiveTo,
      onSelect,
      optionCount,
      optionRowIndices,
      rows,
      showBehavior,
    ]
  )
}
