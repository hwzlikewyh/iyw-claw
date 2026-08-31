import { ModelBehaviorMenu } from "@/components/chat/model-behavior-menu"
import type { SessionConfigOptionInfo } from "@/lib/types"

interface ModelBehaviorPreviewPanelProps {
  modelValue: string | null
  options: SessionConfigOptionInfo[]
  onSelect: (modelValue: string, configId: string, valueId: string) => void
  compact: boolean
}

export function ModelBehaviorPreviewPanel({
  modelValue,
  options,
  onSelect,
  compact,
}: ModelBehaviorPreviewPanelProps) {
  if (!modelValue || options.length === 0) return null
  return (
    <ModelBehaviorMenu
      options={options}
      onSelect={(configId, valueId) =>
        onSelect(modelValue, configId, valueId)
      }
      compact={compact}
    />
  )
}
