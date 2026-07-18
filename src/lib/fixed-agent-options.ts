import type { AgentOptionsSnapshot, AgentType } from "@/lib/types"
import {
  buildAgentOptionsSnapshot,
  getCachedGatewayModels,
  getGatewayModels,
  refreshGatewayModels,
} from "@/lib/gateway-model-catalog"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"

/**
 * Synchronous product-owned selector snapshot. The gateway cache is optional:
 * callers always receive local models and modes on the first render.
 */
export function getFixedAgentOptions(
  agentType: AgentType,
  configValues: Record<string, string> = {},
  translator?: SessionConfigTranslator
): AgentOptionsSnapshot {
  const snapshot = buildAgentOptionsSnapshot(agentType, configValues)
  return translator
    ? {
        ...snapshot,
        config_options: snapshot.config_options.map((option) =>
          localizeSessionConfigOption(option, translator)
        ),
      }
    : snapshot
}

/** Start a best-effort refresh without making selector consumers await it. */
export function loadFixedAgentOptions(): Promise<unknown> {
  return getGatewayModels()
}

export function refreshFixedAgentOptions(): Promise<unknown> {
  return refreshGatewayModels()
}

export function hasCachedFixedAgentOptions(): boolean {
  return getCachedGatewayModels().length > 0
}
