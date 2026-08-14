import {
  buildAgentOptionsSnapshot,
  getCachedGatewayModels,
  getGatewayModels,
  hasAuthoritativeGatewayModels,
  refreshGatewayModels,
} from "@/lib/gateway-model-catalog"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"
import type { AgentOptionsSnapshot, AgentType } from "@/lib/types"

export function getFixedAgentOptions(
  agentType: AgentType,
  configValues: Record<string, string> = {},
  translator?: SessionConfigTranslator
): AgentOptionsSnapshot {
  const snapshot = buildAgentOptionsSnapshot(
    agentType,
    getCachedGatewayModels(agentType),
    configValues
  )
  return translator
    ? {
        ...snapshot,
        config_options: snapshot.config_options.map((option) =>
          localizeSessionConfigOption(option, translator)
        ),
      }
    : snapshot
}

export function loadFixedAgentOptions(agentType: AgentType): Promise<unknown> {
  return getGatewayModels(agentType)
}

export function refreshFixedAgentOptions(
  agentType: AgentType
): Promise<unknown> {
  return refreshGatewayModels(agentType)
}

export function hasCachedFixedAgentOptions(agentType: AgentType): boolean {
  return getCachedGatewayModels(agentType).length > 0
}

export function hasAuthoritativeFixedAgentOptions(
  agentType: AgentType
): boolean {
  return hasAuthoritativeGatewayModels(agentType)
}
