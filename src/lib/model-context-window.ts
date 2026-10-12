import type { GatewayModel } from "@/lib/gateway-model-parser"
import type { AgentType, SessionConfigOptionInfo } from "@/lib/types"
import { isCustomAgentType } from "@/lib/types"

export const CONTEXT_WINDOW_CONFIG_ID = "context_window"
const TOKENS_PER_K = 1_000
const TOKENS_PER_M = 1_000_000

export function contextWindowLabel(tokens: number): string {
  return tokens >= TOKENS_PER_M
    ? `${tokens / TOKENS_PER_M}M`
    : `${tokens / TOKENS_PER_K}K`
}

function contextWindowOptions(model: GatewayModel, maximum: number) {
  const tiers = [
    ...new Set((model.contextWindowTiers ?? []).filter((v) => v <= maximum)),
  ].sort((left, right) => left - right)
  const automatic = model.defaultContextWindow || maximum
  return [
    {
      value: "auto",
      name: `Auto · ${contextWindowLabel(Math.min(automatic, maximum))}`,
      description: model.id,
    },
    ...tiers.map((tokens) => ({
      value: String(tokens),
      name: contextWindowLabel(tokens),
      description: null,
    })),
  ]
}

export function buildContextWindowOption({
  agentType,
  model,
  configured,
}: {
  agentType?: AgentType | null
  model?: GatewayModel | null
  configured?: string
}): SessionConfigOptionInfo | null {
  const maximum = model?.contextWindow
  if (
    !agentType ||
    agentType === "cursor" ||
    agentType === "deepseek" ||
    isCustomAgentType(agentType) ||
    !maximum
  )
    return null
  const options = contextWindowOptions(model, maximum)
  return {
    id: CONTEXT_WINDOW_CONFIG_ID,
    name: "Context window",
    category: "model_config",
    kind: {
      type: "select",
      current_value: options.some(({ value }) => value === configured)
        ? configured!
        : "auto",
      options,
      groups: [],
    },
  }
}
