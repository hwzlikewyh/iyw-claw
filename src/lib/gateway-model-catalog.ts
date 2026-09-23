import { getAgentModeState } from "@/lib/agent-modes"
import {
  getAgentModelBehaviorIds,
  getStaticAgentConfigOptions,
} from "@/lib/agent-control-profiles"
import { listGatewayModels } from "@/lib/api"
import type {
  AgentOptionsSnapshot,
  AgentType,
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"
import type { GatewayModel } from "@/lib/gateway-model-parser"
import {
  browserPayloadCache,
  createGatewayModelCatalog,
  type GatewayModelCatalog,
} from "@/lib/gateway-model-store"

export { parseGatewayModels } from "@/lib/gateway-model-parser"
export type {
  GatewayImageInputMode,
  GatewayModel,
  GatewayModelCapabilities,
} from "@/lib/gateway-model-parser"
export {
  createGatewayModelCatalog,
  type GatewayModelCatalog,
  type GatewayModelPayloadCache,
} from "@/lib/gateway-model-store"

const GATEWAY_MODEL_CACHE_KEY = "iyw-claw.gateway-model-catalog.v1"

function selectOption(
  value: string,
  name: string,
  description: string | null,
  iconUrl: string | null = null
): SessionConfigSelectOptionInfo {
  return { value, name, description, iconUrl }
}

function effortLabel(effort: string): string {
  const normalized = effort.trim().toLowerCase().replace(/_/g, "-")
  if (normalized === "xhigh" || normalized === "extra-high") {
    return "Extra high"
  }
  if (normalized === "max") return "Maximum"
  return effort.charAt(0).toUpperCase() + effort.slice(1)
}

function buildReasoningOptions(
  model: GatewayModel
): SessionConfigSelectOptionInfo[] {
  return model.efforts.map((effort) =>
    selectOption(effort, effortLabel(effort), null)
  )
}

function buildModelOption(
  selectedId: string,
  models: GatewayModel[],
  reasoningConfigId: string | null,
  modelConfigValues: Record<string, Record<string, string>>
): SessionConfigOptionInfo {
  return {
    id: "model",
    name: "Model",
    description: "Choose the model for this session.",
    category: "model",
    kind: {
      type: "select",
      current_value: selectedId,
      options: models.map((model) => ({
        ...selectOption(model.id, model.name, model.description, model.iconUrl),
        priceMultiplier: model.priceMultiplier,
        modelBehavior: {
          reasoningOptions: buildReasoningOptions(model),
          defaultReasoningEffort: model.defaultEffort,
          savedReasoningEffort: reasoningConfigId
            ? (modelConfigValues[model.id]?.[reasoningConfigId] ??
              modelConfigValues[model.id]?.reasoning_effort ??
              null)
            : null,
          fastModeSupported: model.fastModeSupported,
          fastModeDefaultEnabled: model.fastModeDefaultEnabled,
        },
      })),
      groups: [],
    },
  }
}

export function resolveModelReasoningEffort(
  model: SessionConfigSelectOptionInfo,
  savedValue?: string
): string | null {
  const metadata = model.modelBehavior
  if (!metadata || metadata.reasoningOptions.length === 0) return null
  if (
    savedValue &&
    metadata.reasoningOptions.some((option) => option.value === savedValue)
  ) {
    return savedValue
  }
  if (
    metadata.defaultReasoningEffort &&
    metadata.reasoningOptions.some(
      (option) => option.value === metadata.defaultReasoningEffort
    )
  ) {
    return metadata.defaultReasoningEffort
  }
  return metadata.reasoningOptions[0].value
}

function buildEffortOption(
  selected: GatewayModel,
  id: string,
  configuredEffort: string | undefined
): SessionConfigOptionInfo | null {
  if (selected.efforts.length === 0) return null
  const current = selected.efforts.includes(configuredEffort ?? "")
    ? configuredEffort!
    : selected.defaultEffort &&
        selected.efforts.includes(selected.defaultEffort)
      ? selected.defaultEffort
      : selected.efforts[0]
  return {
    id,
    name: "Reasoning effort",
    description: "Adjust how deeply the model reasons before responding.",
    category: "thought_level",
    kind: {
      type: "select",
      current_value: current,
      options: selected.efforts.map((effort) =>
        selectOption(effort, effortLabel(effort), null)
      ),
      groups: [],
    },
  }
}

function buildFastModeOption(
  selected: GatewayModel,
  id: string,
  configuredValue: string | undefined
): SessionConfigOptionInfo | null {
  if (!selected.fastModeSupported) return null
  const current =
    configuredValue === "on" || configuredValue === "off"
      ? configuredValue
      : selected.fastModeDefaultEnabled
        ? "on"
        : "off"
  return {
    id,
    name: "Fast mode",
    description: "Choose the response speed for this session.",
    category: "model_config",
    kind: {
      type: "select",
      current_value: current,
      options: [
        selectOption("off", "Off", "Standard response speed"),
        selectOption("on", "Fast", "Faster responses with additional usage"),
      ],
      groups: [],
    },
  }
}

export function buildAgentOptionsSnapshot(
  agentType: AgentType,
  models: GatewayModel[],
  configValues: Record<string, string> = {},
  modelConfigValues: Record<string, Record<string, string>> = {}
): AgentOptionsSnapshot {
  const selectedId = configValues.model || models[0]?.id
  const selected = models.find((model) => model.id === selectedId)
  const configOptions: SessionConfigOptionInfo[] = []
  const behavior = getAgentModelBehaviorIds(agentType)
  if (selectedId) {
    configOptions.push(
      buildModelOption(
        selectedId,
        models,
        behavior.effortConfigId,
        modelConfigValues
      )
    )
  }
  if (selected) {
    if (behavior.effortConfigId) {
      const configured =
        modelConfigValues[selected.id]?.[behavior.effortConfigId] ??
        modelConfigValues[selected.id]?.reasoning_effort
      const effort = buildEffortOption(
        selected,
        behavior.effortConfigId,
        configured
      )
      if (effort) configOptions.push(effort)
    }
    if (behavior.fastConfigId) {
      const fastMode = buildFastModeOption(
        selected,
        behavior.fastConfigId,
        configValues[behavior.fastConfigId]
      )
      if (fastMode) configOptions.push(fastMode)
    }
  }
  configOptions.push(...getStaticAgentConfigOptions(agentType, configValues))
  return {
    modes: getAgentModeState(agentType),
    config_options: configOptions,
    available_commands: [],
  }
}

const MODEL_CONFIG_IDS = [
  "model",
  "reasoning_effort",
  "effort",
  "thought_level",
  "fast-mode",
  "fast",
  "fast_mode",
]

export function hasModelConfigValues(
  configValues: Record<string, string>
): boolean {
  return MODEL_CONFIG_IDS.some((id) => id in configValues)
}

export function reconcileModelConfigValues(
  snapshot: AgentOptionsSnapshot,
  configValues: Record<string, string>
): Record<string, string> {
  const model = snapshot.config_options.find((option) => option.id === "model")
  // 目录变化不能替用户改选模型；不可用的选择保留到用户手动修改。
  if (
    configValues.model &&
    !model?.kind.options.some((item) => item.value === configValues.model)
  ) {
    return configValues
  }
  const next = { ...configValues }
  if (!model) {
    for (const id of MODEL_CONFIG_IDS) delete next[id]
  } else {
    for (const id of MODEL_CONFIG_IDS) {
      const option = snapshot.config_options.find((item) => item.id === id)
      if (!option) {
        delete next[id]
        continue
      }
      if (!option.kind.options.some((item) => item.value === next[id])) {
        next[id] = option.kind.current_value
      }
    }
  }
  const keys = Object.keys(configValues)
  const unchanged =
    keys.length === Object.keys(next).length &&
    keys.every((key) => configValues[key] === next[key])
  return unchanged ? configValues : next
}

const gatewayModelCatalog = createGatewayModelCatalog({
  fetchModels: listGatewayModels,
  cache: browserPayloadCache(GATEWAY_MODEL_CACHE_KEY),
})
const agentModelCatalogs = new Map<AgentType, GatewayModelCatalog>()

function catalogFor(agentType?: AgentType): GatewayModelCatalog {
  if (!agentType) return gatewayModelCatalog
  const existing = agentModelCatalogs.get(agentType)
  if (existing) return existing
  const catalog = createGatewayModelCatalog({
    fetchModels: () => listGatewayModels(agentType),
    cache: browserPayloadCache(`${GATEWAY_MODEL_CACHE_KEY}.sdk.${agentType}`),
    replaceWithEmpty: true,
  })
  agentModelCatalogs.set(agentType, catalog)
  return catalog
}

// ── Periodic auto-refresh ───
//
// The catalog used to be fetched once per app session, so a long-running
// desktop app never saw gateway-side model additions/removals until restart.
// Arm a background interval on the first catalog access (browser only —
// never during static export/SSR); consumers keep reading through
// getCachedGatewayModels() and naturally pick up refreshed data.
const AUTO_REFRESH_INTERVAL_MS = 30 * 60_000

let autoRefreshTimer: ReturnType<typeof setInterval> | null = null

function ensureAutoRefresh(): void {
  if (autoRefreshTimer !== null || typeof window === "undefined") return
  autoRefreshTimer = setInterval(() => {
    void gatewayModelCatalog.refresh()
    for (const catalog of agentModelCatalogs.values()) void catalog.refresh()
  }, AUTO_REFRESH_INTERVAL_MS)
}

export function getCachedGatewayModels(agentType?: AgentType): GatewayModel[] {
  return catalogFor(agentType).getCached()
}

export function hasAuthoritativeGatewayModels(agentType?: AgentType): boolean {
  return catalogFor(agentType).hasAuthoritativeData()
}

export function getGatewayModels(
  agentType?: AgentType
): Promise<GatewayModel[]> {
  ensureAutoRefresh()
  return catalogFor(agentType).load()
}

export function refreshGatewayModels(
  agentType?: AgentType
): Promise<GatewayModel[]> {
  ensureAutoRefresh()
  return catalogFor(agentType).refresh()
}
