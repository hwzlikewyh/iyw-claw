import { isModelConfigOption } from "@/lib/model-config-groups"
import type {
  AgentType,
  ModelSwitchCapability,
  SessionConfigOptionInfo,
} from "@/lib/types"

const MANAGED_GATEWAY_AGENTS = new Set<AgentType>([
  "claude_code",
  "codex",
  "open_code",
  "open_claw",
  "cline",
  "hermes",
  "code_buddy",
  "kimi_code",
  "pi",
  "grok",
])

const DEFAULT_MODEL_REAPPLY_TIMEOUT_MS = 60_000
const SLOW_MODEL_REAPPLY_TIMEOUT_MS = 120_000

export function hasManagedGatewayModelProjection(
  agentType: AgentType
): boolean {
  return MANAGED_GATEWAY_AGENTS.has(agentType)
}

export function modelReapplyTimeoutMs(agentType: AgentType): number {
  return agentType === "pi"
    ? SLOW_MODEL_REAPPLY_TIMEOUT_MS
    : DEFAULT_MODEL_REAPPLY_TIMEOUT_MS
}

export function liveAgentConfigOptions(
  agentType: AgentType,
  fixedOptions: SessionConfigOptionInfo[],
  liveOptions: SessionConfigOptionInfo[] | null,
  selectorsReady: boolean,
  capability: ModelSwitchCapability
): SessionConfigOptionInfo[] {
  if (!selectorsReady) {
    return hasManagedGatewayModelProjection(agentType)
      ? fixedOptions
      : fixedOptions.filter((option) => !isModelConfigOption(option))
  }
  if (!liveOptions) return []

  if (
    hasManagedGatewayModelProjection(agentType) &&
    capability !== "interactive"
  ) {
    return liveOptions.filter((option) => !isModelConfigOption(option))
  }

  const liveModel = liveOptions.find(isModelConfigOption)
  if (!liveModel || !hasManagedGatewayModelProjection(agentType)) {
    return liveOptions
  }
  const fixedModel = fixedOptions.find(isModelConfigOption)
  if (!fixedModel) {
    return liveOptions.filter((option) => !isModelConfigOption(option))
  }

  const selectable = [...fixedModel.kind.options]
  if (!selectable.some(({ value }) => value === liveModel.kind.current_value)) {
    const current = liveModel.kind.options.find(
      ({ value }) => value === liveModel.kind.current_value
    )
    if (current) selectable.unshift(current)
  }
  const mergedModel: SessionConfigOptionInfo = {
    ...fixedModel,
    kind: {
      ...fixedModel.kind,
      current_value: liveModel.kind.current_value,
      options: selectable,
    },
  }
  return liveOptions.map((option) =>
    isModelConfigOption(option) ? mergedModel : option
  )
}
