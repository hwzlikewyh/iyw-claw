import {
  buildAgentOptionsSnapshot,
  getCachedGatewayModels,
  refreshGatewayModels,
} from "@/lib/gateway-model-catalog"
import { deriveAgentModels } from "@/lib/agent-option-definitions"
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
  // Preserve the gateway response exactly. The fusion layer owns protocol
  // conversion and the response has no per-agent field to filter on here.
  const snapshot = buildAgentOptionsSnapshot(
    agentType,
    deriveAgentModels(agentType, getCachedGatewayModels()),
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

export function loadFixedAgentOptions(): Promise<unknown> {
  return refreshGatewayModels()
}

export function refreshFixedAgentOptions(): Promise<unknown> {
  return refreshGatewayModels()
}

export function hasCachedFixedAgentOptions(): boolean {
  return getCachedGatewayModels().length > 0
}
