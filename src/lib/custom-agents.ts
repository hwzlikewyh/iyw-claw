import {
  AGENT_COLORS,
  AGENT_LABELS,
  customAgentId,
  isCustomAgentType,
  type AcpAgentInfo,
  type AgentType,
  type CustomAgentType,
} from "@/lib/types"

type AgentDisplayEntry = Pick<AcpAgentInfo, "agent_type" | "name">

const customAgentNames = new Map<CustomAgentType, string>()

export function refreshCustomAgentNames(
  entries: readonly AgentDisplayEntry[]
): void {
  customAgentNames.clear()
  for (const entry of entries) {
    if (!isCustomAgentType(entry.agent_type)) continue
    const name = entry.name.trim()
    if (name) customAgentNames.set(entry.agent_type, name)
  }
}

function humanizeRegistryId(registryId: string): string {
  return registryId
    .split(/[-_.]/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ")
}

export function getAgentLabel(agentType: AgentType): string {
  if (!isCustomAgentType(agentType)) return AGENT_LABELS[agentType]
  const registryId = customAgentId(agentType)
  return (
    customAgentNames.get(agentType) ??
    (registryId ? humanizeRegistryId(registryId) : agentType)
  )
}

const CUSTOM_AGENT_COLORS = [
  "bg-sky-600",
  "bg-teal-600",
  "bg-rose-600",
  "bg-amber-600",
  "bg-indigo-600",
  "bg-lime-600",
  "bg-fuchsia-600",
] as const

const HASH_MULTIPLIER = 31

function hashRegistryId(registryId: string): number {
  let hash = 0
  for (const character of registryId) {
    hash = (hash * HASH_MULTIPLIER + character.charCodeAt(0)) >>> 0
  }
  return hash
}

export function getAgentColor(agentType: AgentType): string {
  if (!isCustomAgentType(agentType)) return AGENT_COLORS[agentType]
  const registryId = customAgentId(agentType) ?? agentType
  return CUSTOM_AGENT_COLORS[
    hashRegistryId(registryId) % CUSTOM_AGENT_COLORS.length
  ]
}

export function getAgentInitial(agentType: AgentType): string {
  return getAgentLabel(agentType).trim().charAt(0).toUpperCase() || "?"
}
