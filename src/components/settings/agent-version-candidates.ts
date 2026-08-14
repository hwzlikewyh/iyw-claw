import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type {
  AgentType,
  AgentVersionHistory,
  AgentVersionHistoryItem,
  AgentVersionInventory,
} from "@/lib/types"

export function mergeVersionHistory(
  history: AgentVersionHistory,
  agentType: AgentType,
  inventory?: AgentVersionInventory,
  registryVersion?: string | null
): AgentVersionHistory {
  const items = new Map(history.items.map((item) => [item.version, item]))
  const fallbackVersions = new Set([
    ...(inventory?.installations ?? [])
      .filter((item) => item.verified)
      .map((item) => item.version),
    ...(registryVersion ? [registryVersion] : []),
  ])
  for (const version of fallbackVersions) {
    if (!isVersionLike(version) || items.has(version)) continue
    items.set(version, fallbackHistoryItem(agentType, version, inventory))
  }
  return {
    items: [...items.values()].sort((left, right) =>
      compareVersions(right.version, left.version)
    ),
  }
}

function fallbackHistoryItem(
  agentType: AgentType,
  version: string,
  inventory?: AgentVersionInventory
): AgentVersionHistoryItem {
  return {
    id: `fallback:${agentType}:${version}`,
    version,
    title: `${getAgentDisplayName(agentType)} ${version}`,
    notesMarkdown: "",
    channel: inventory?.updateChannel ?? "stable",
    lifecycleStatus: "published",
    securityStatus: "unknown",
    updatePolicy: inventory?.updatePolicy ?? "manual",
    publishedAt: null,
    rolloutEligible: true,
    recommended: false,
    minimumSafe: false,
    pinnable: false,
    deliveryKind: "registry",
    artifactId: "",
  }
}

function isVersionLike(value: string): boolean {
  return /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(value.trim())
}

function compareVersions(left: string, right: string): number {
  const parse = (value: string) =>
    value
      .replace(/^[^\d]*/, "")
      .split(/[.+-]/)
      .map((part) => Number.parseInt(part, 10) || 0)
  const a = parse(left)
  const b = parse(right)
  const length = Math.max(a.length, b.length)
  for (let index = 0; index < length; index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0)
    if (difference !== 0) return difference
  }
  return left.localeCompare(right)
}
