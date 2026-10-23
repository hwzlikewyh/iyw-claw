import { Box } from "lucide-react"
import { ModelIcon } from "@/components/chat/model-icon"
import type { SessionConfigOptionInfo } from "@/lib/types"
import {
  isModelConfigOption,
  modelDisplayName,
} from "@/lib/model-config-groups"

export function TurnModelLabel({
  model,
  options,
}: {
  model: string
  options?: SessionConfigOptionInfo[]
}) {
  const config = options?.find(isModelConfigOption)
  const choices = config
    ? [
        ...config.kind.options,
        ...config.kind.groups.flatMap((group) => group.options),
      ]
    : []
  const selected =
    choices.find((option) => option.value === model || option.name === model) ??
    [...choices]
      .sort((a, b) => b.value.length - a.value.length)
      .find((option) => model.startsWith(option.value + "-"))
  return (
    <span
      className="inline-flex min-h-8 min-w-0 max-w-full items-center gap-1.5 px-1"
      translate="no"
    >
      {selected?.iconUrl ? (
        <ModelIcon src={selected.iconUrl} className="size-3.5" />
      ) : (
        <Box aria-hidden="true" className="size-3.5 shrink-0" />
      )}
      <span className="break-words">{modelDisplayName(options, model)}</span>
    </span>
  )
}
