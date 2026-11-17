import { Box } from "lucide-react"
import { ModelIcon } from "@/components/chat/model-icon"
import type { GatewayModel } from "@/lib/gateway-model-parser"
import { findUsageModel } from "@/lib/session-usage-display"
import type { SessionConfigOptionInfo } from "@/lib/types"
import {
  isModelConfigOption,
  modelDisplayName,
} from "@/lib/model-config-groups"

function findModelAlias(
  models: GatewayModel[],
  value: string,
  lastTurnModelId?: string | null
) {
  const terms = value.toLowerCase().match(/[a-z0-9]+/g) ?? []
  if (terms.length < 2) return null
  const matches = models.filter((item) => {
    const words = item.id.toLowerCase().match(/[a-z0-9]+/g) ?? []
    let next = 0
    for (const word of words) {
      if (word === terms[next]) next += 1
    }
    return next === terms.length
  })
  if (matches.length === 1) return matches[0]
  return findUsageModel(matches, lastTurnModelId ?? null)
}

export function TurnModelLabel({
  model,
  options,
  gatewayModels,
  lastTurnModelId,
}: {
  model: string
  options?: SessionConfigOptionInfo[]
  gatewayModels?: GatewayModel[]
  lastTurnModelId?: string | null
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
  const catalog = gatewayModels ?? []
  const catalogModel =
    findUsageModel(catalog, model.trim()) ??
    findModelAlias(catalog, model, lastTurnModelId)
  const selectedName = selected?.name.trim()
  const displayName =
    (selectedName && selectedName !== selected?.value ? selectedName : null) ||
    catalogModel?.name ||
    modelDisplayName(options, model)
  return (
    <span
      className="inline-flex min-h-8 min-w-0 max-w-full items-center gap-1.5 px-1"
      translate="no"
    >
      {selected?.iconUrl || catalogModel?.iconUrl ? (
        <ModelIcon
          src={selected?.iconUrl || catalogModel?.iconUrl || ""}
          className="size-3.5"
        />
      ) : (
        <Box aria-hidden="true" className="size-3.5 shrink-0" />
      )}
      <span className="break-words">{displayName}</span>
    </span>
  )
}
