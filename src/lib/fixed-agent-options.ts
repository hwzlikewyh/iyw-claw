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
import { getSavedModelConfigPreferences } from "@/lib/selector-prefs-storage"
import type { AgentOptionsSnapshot, AgentType } from "@/lib/types"

export function getFixedAgentOptions(
  agentType: AgentType,
  configValues: Record<string, string> = {},
  translator?: SessionConfigTranslator
): AgentOptionsSnapshot {
  const snapshot = buildAgentOptionsSnapshot(
    agentType,
    getCachedGatewayModels(agentType),
    configValues,
    getSavedModelConfigPreferences(agentType)
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

export async function loadFixedAgentOptions(
  agentType: AgentType
): Promise<unknown> {
  // 登录后的选择器必须重新取目录，不能沿用上次登录的已加载标记。
  await refreshGatewayModels(agentType)
  // 若复用了登录前失败的请求，load 会立即重试，并发调用仍共享请求。
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
