import { getAgentModeState } from "@/lib/agent-modes"
import { listGatewayModels } from "@/lib/api"
import type {
  AgentOptionsSnapshot,
  AgentType,
  BuiltinAgentType,
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"
import { isCustomAgentType } from "@/lib/types"
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
  description: string | null
): SessionConfigSelectOptionInfo {
  return { value, name, description }
}

function effortLabel(effort: string): string {
  return effort === "xhigh"
    ? "Max"
    : effort.charAt(0).toUpperCase() + effort.slice(1)
}

function buildModelOption(
  selected: GatewayModel,
  models: GatewayModel[]
): SessionConfigOptionInfo {
  return {
    id: "model",
    name: "Model",
    description: "Choose the model for this session.",
    category: "model",
    kind: {
      type: "select",
      current_value: selected.id,
      options: models.map((model) =>
        selectOption(model.id, model.name, model.description)
      ),
      groups: [],
    },
  }
}

function buildEffortOption(
  selected: GatewayModel,
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
    id: "reasoning_effort",
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

const FAST_MODE_CONFIG_IDS: Partial<Record<BuiltinAgentType, string>> = {
  codex: "fast-mode",
  claude_code: "fast",
}

function buildFastModeOption(
  selected: GatewayModel,
  agentType: AgentType,
  configuredValue: string | undefined
): SessionConfigOptionInfo | null {
  if (!selected.fastModeSupported) return null
  const id = isCustomAgentType(agentType)
    ? undefined
    : FAST_MODE_CONFIG_IDS[agentType]
  if (!id) return null
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
  configValues: Record<string, string> = {}
): AgentOptionsSnapshot {
  const selected =
    models.find((model) => model.id === configValues.model) ?? models[0]
  const configOptions: SessionConfigOptionInfo[] = []
  if (selected) {
    configOptions.push(buildModelOption(selected, models))
    const effort = buildEffortOption(selected, configValues.reasoning_effort)
    if (effort) configOptions.push(effort)
    const fastModeConfigId = isCustomAgentType(agentType)
      ? undefined
      : FAST_MODE_CONFIG_IDS[agentType]
    const fastMode = buildFastModeOption(
      selected,
      agentType,
      configValues[fastModeConfigId ?? ""]
    )
    if (fastMode) configOptions.push(fastMode)
  }
  return {
    modes: getAgentModeState(agentType),
    config_options: configOptions,
    available_commands: [],
  }
}

const MODEL_CONFIG_IDS = [
  "model",
  "reasoning_effort",
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
