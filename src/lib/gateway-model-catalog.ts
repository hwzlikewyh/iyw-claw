import { getAgentModeState } from "@/lib/agent-modes"
import { listGatewayModels } from "@/lib/api"
import type {
  AgentOptionsSnapshot,
  AgentType,
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"

const GATEWAY_MODEL_CACHE_KEY = "iyw-claw.gateway-model-catalog.v1"

export interface GatewayModel {
  id: string
  name: string
  description: string | null
  efforts: string[]
  defaultEffort: string | null
  fastModeSupported: boolean
  fastModeDefaultEnabled: boolean
}

export interface GatewayModelPayloadCache {
  read: () => unknown | null
  write: (payload: unknown) => void
}

interface GatewayModelCatalogOptions {
  fetchModels: () => Promise<unknown>
  cache: GatewayModelPayloadCache
}

export interface GatewayModelCatalog {
  getCached: () => GatewayModel[]
  load: () => Promise<GatewayModel[]>
  refresh: () => Promise<GatewayModel[]>
}

function uniqueStrings(value: unknown): string[] {
  if (!Array.isArray(value)) return []
  return Array.from(
    new Set(
      value.flatMap((item) => {
        if (typeof item !== "string") return []
        const trimmed = item.trim()
        return trimmed ? [trimmed] : []
      })
    )
  )
}

function parseGatewayModel(value: unknown): GatewayModel | null {
  if (!value || typeof value !== "object") return null
  const raw = value as Record<string, unknown>
  const id = typeof raw.id === "string" ? raw.id.trim() : ""
  if (!id) return null
  const reasoning =
    raw.reasoning && typeof raw.reasoning === "object"
      ? (raw.reasoning as Record<string, unknown>)
      : {}
  const efforts = uniqueStrings(reasoning.efforts)
  const defaultEffort =
    typeof reasoning.default_effort === "string" &&
    reasoning.default_effort.trim()
      ? reasoning.default_effort.trim()
      : null
  const fastMode =
    raw.fast_mode && typeof raw.fast_mode === "object"
      ? (raw.fast_mode as Record<string, unknown>)
      : {}
  return {
    id,
    name:
      typeof raw.display_name === "string" && raw.display_name.trim()
        ? raw.display_name.trim()
        : id,
    description:
      typeof raw.description === "string" && raw.description.trim()
        ? raw.description.trim()
        : null,
    efforts,
    defaultEffort,
    fastModeSupported: fastMode.supported === true,
    fastModeDefaultEnabled: fastMode.default_enabled === true,
  }
}

export function parseGatewayModels(payload: unknown): GatewayModel[] {
  if (!payload || typeof payload !== "object") return []
  const data = (payload as { data?: unknown }).data
  if (!Array.isArray(data)) return []
  const seen = new Set<string>()
  return data.flatMap((item) => {
    const model = parseGatewayModel(item)
    if (!model || seen.has(model.id)) return []
    seen.add(model.id)
    return [model]
  })
}

function browserPayloadCache(): GatewayModelPayloadCache {
  return {
    read: () => {
      try {
        const raw = globalThis.localStorage?.getItem(GATEWAY_MODEL_CACHE_KEY)
        return raw ? JSON.parse(raw) : null
      } catch {
        return null
      }
    },
    write: (payload) => {
      try {
        globalThis.localStorage?.setItem(
          GATEWAY_MODEL_CACHE_KEY,
          JSON.stringify(payload)
        )
      } catch {
        // The in-memory online cache remains available for this app session.
      }
    },
  }
}

export function createGatewayModelCatalog({
  fetchModels,
  cache,
}: GatewayModelCatalogOptions): GatewayModelCatalog {
  let cached = parseGatewayModels(cache.read())
  let loaded = false
  let pending: Promise<GatewayModel[]> | null = null

  const refresh = (): Promise<GatewayModel[]> => {
    if (pending) return pending
    pending = fetchModels()
      .then((payload) => {
        const online = parseGatewayModels(payload)
        if (online.length > 0) {
          cached = online
          cache.write(payload)
        }
        return [...cached]
      })
      .catch(() => [...cached])
      .finally(() => {
        loaded = true
        pending = null
      })
    return pending
  }

  return {
    getCached: () => [...cached],
    load: () => (loaded ? Promise.resolve([...cached]) : refresh()),
    refresh,
  }
}

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

const FAST_MODE_CONFIG_IDS: Partial<Record<AgentType, string>> = {
  codex: "fast-mode",
  claude_code: "fast",
}

function buildFastModeOption(
  selected: GatewayModel,
  agentType: AgentType,
  configuredValue: string | undefined
): SessionConfigOptionInfo | null {
  if (!selected.fastModeSupported) return null
  const id = FAST_MODE_CONFIG_IDS[agentType]
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
    const fastMode = buildFastModeOption(
      selected,
      agentType,
      configValues[FAST_MODE_CONFIG_IDS[agentType] ?? ""]
    )
    if (fastMode) configOptions.push(fastMode)
  }
  return {
    modes: getAgentModeState(agentType),
    config_options: configOptions,
    available_commands: [],
  }
}

export function reconcileModelConfigValues(
  snapshot: AgentOptionsSnapshot,
  configValues: Record<string, string>
): Record<string, string> {
  const model = snapshot.config_options.find((option) => option.id === "model")
  if (!model) return configValues
  const next = { ...configValues }
  for (const id of [
    "model",
    "reasoning_effort",
    "fast-mode",
    "fast",
    "fast_mode",
  ]) {
    const option = snapshot.config_options.find((item) => item.id === id)
    if (!option) {
      delete next[id]
      continue
    }
    if (!option.kind.options.some((item) => item.value === next[id])) {
      next[id] = option.kind.current_value
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
  cache: browserPayloadCache(),
})

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
  }, AUTO_REFRESH_INTERVAL_MS)
}

export function getCachedGatewayModels(): GatewayModel[] {
  return gatewayModelCatalog.getCached()
}

export function getGatewayModels(): Promise<GatewayModel[]> {
  ensureAutoRefresh()
  return gatewayModelCatalog.load()
}

export function refreshGatewayModels(): Promise<GatewayModel[]> {
  ensureAutoRefresh()
  return gatewayModelCatalog.refresh()
}
