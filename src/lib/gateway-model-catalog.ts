import { listGatewayModels } from "@/lib/api"
import {
  getAgentModeState,
  getLocalAgentModelIds,
  getLocalModels,
} from "@/lib/agent-option-definitions"
import type {
  AgentOptionsSnapshot,
  AgentType,
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"

export { getLocalAgentModelIds }

export interface GatewayModel {
  id: string
  name: string
  description: string | null
  efforts: string[]
  defaultEffort: string | null
}

export interface AgentModelDefinition extends GatewayModel {
  source: "local" | "gateway"
}

let cachedGatewayModels: GatewayModel[] | null = null
let refreshPromise: Promise<GatewayModel[]> | null = null

export function parseGatewayModels(payload: unknown): GatewayModel[] {
  if (!payload || typeof payload !== "object") return []
  const data = (payload as { data?: unknown }).data
  if (!Array.isArray(data)) return []

  return data.flatMap((item): GatewayModel[] => {
    if (!item || typeof item !== "object") return []
    const raw = item as {
      id?: unknown
      display_name?: unknown
      description?: unknown
      reasoning?: { efforts?: unknown; default_effort?: unknown }
    }
    const id = typeof raw.id === "string" ? raw.id.trim() : ""
    if (!id) return []
    const efforts = Array.isArray(raw.reasoning?.efforts)
      ? raw.reasoning.efforts.filter(
          (effort): effort is string =>
            typeof effort === "string" && effort.trim().length > 0
        )
      : []
    return [
      {
        id,
        name:
          typeof raw.display_name === "string" && raw.display_name.trim()
            ? raw.display_name
            : id,
        description:
          typeof raw.description === "string" ? raw.description : null,
        efforts,
        defaultEffort:
          typeof raw.reasoning?.default_effort === "string"
            ? raw.reasoning.default_effort
            : null,
      },
    ]
  })
}

export function mergeAgentModels(
  agentType: AgentType,
  remoteModels: GatewayModel[]
): AgentModelDefinition[] {
  const remoteById = new Map(remoteModels.map((model) => [model.id, model]))
  return getLocalModels(agentType).map((local) => {
    const remote = remoteById.get(local.id)
    const efforts = remote?.efforts.length ? remote.efforts : local.efforts
    const remoteDefault = remote?.defaultEffort
    return {
      ...(remote ?? local),
      source: remote ? "gateway" : "local",
      efforts,
      defaultEffort:
        remoteDefault && efforts.includes(remoteDefault)
          ? remoteDefault
          : local.defaultEffort,
    }
  })
}

export async function getGatewayModels(): Promise<GatewayModel[]> {
  if (cachedGatewayModels?.length) return cachedGatewayModels
  if (!refreshPromise) {
    refreshPromise = listGatewayModels()
      .then(parseGatewayModels)
      .catch(() => [])
      .then((models) => {
        cachedGatewayModels = models.length > 0 ? models : null
        return models
      })
      .finally(() => {
        refreshPromise = null
      })
  }
  return refreshPromise
}

export function getCachedGatewayModels(): GatewayModel[] {
  return cachedGatewayModels ?? []
}

export function refreshGatewayModels(): Promise<GatewayModel[]> {
  cachedGatewayModels = null
  return getGatewayModels()
}

function option(value: string, name: string, description: string | null) {
  return { value, name, description } satisfies SessionConfigSelectOptionInfo
}

function buildModelOption(
  model: AgentModelDefinition,
  models: AgentModelDefinition[]
): SessionConfigOptionInfo {
  return {
    id: "model",
    name: "Model",
    description: "Choose the model for this session.",
    category: "model",
    kind: {
      type: "select",
      current_value: model.id,
      options: models.map((item) =>
        option(item.id, item.name, item.description)
      ),
      groups: [],
    },
  }
}

function effortLabel(effort: string): string {
  return effort === "xhigh" ? "Max" : effort[0].toUpperCase() + effort.slice(1)
}

export function buildAgentOptionsSnapshot(
  agentType: AgentType,
  configValues: Record<string, string> = {}
): AgentOptionsSnapshot {
  const models = mergeAgentModels(agentType, getCachedGatewayModels())
  const selected =
    models.find((model) => model.id === configValues.model) ?? models[0]
  const configOptions: SessionConfigOptionInfo[] = selected
    ? [buildModelOption(selected, models)]
    : []
  const effortOptions =
    selected?.efforts.map((effort) =>
      option(effort, effortLabel(effort), null)
    ) ?? []
  if (selected && effortOptions.length > 0) {
    const selectedEffort = effortOptions.some(
      (effort) => effort.value === configValues.reasoning_effort
    )
      ? configValues.reasoning_effort
      : (selected.defaultEffort ?? effortOptions[0].value)
    configOptions.push({
      id: "reasoning_effort",
      name: "Reasoning effort",
      description: "Adjust how deeply the model reasons before responding.",
      category: "thought_level",
      kind: {
        type: "select",
        current_value: selectedEffort,
        options: effortOptions,
        groups: [],
      },
    })
  }
  return {
    modes: getAgentModeState(agentType),
    config_options: configOptions,
    available_commands: [],
  }
}
