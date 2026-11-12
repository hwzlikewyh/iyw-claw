import type { Transport } from "@/lib/transport"
import type {
  AgentSkillItem,
  AgentType,
  SkillInventorySnapshot,
} from "@/lib/types"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { getSkillMarketSource } from "@/lib/skill-market-source"

export interface ComposerPluginChoice {
  id: string
  slug: string
  label: string
  version: string
  skills: AgentSkillItem[]
  blocked: boolean
  needsPreparation: boolean
}

export async function loadComposerPluginInventory(
  transport: Transport,
  context: { agentType: AgentType; workspacePath: string | null }
) {
  const inventory = await transport.call<SkillInventorySnapshot>(
    "skill_inventory_list",
    { workspacePath: context.workspacePath }
  )
  return { inventory }
}

export function inventoryPluginChoices(
  data: Awaited<ReturnType<typeof loadComposerPluginInventory>>,
  agentType: AgentType
): ComposerPluginChoice[] {
  const groups = new Map<string, ComposerPluginChoice>()
  for (const skill of data.inventory.skills) {
    const observation = skill.observations.find(
      (item) => item.pluginSlug && item.marketSkillId
    )
    if (!observation?.pluginSlug || !observation.marketSkillId) continue
    const id = observation.marketSkillId
    const group = groups.get(id) ?? {
      id,
      slug: observation.pluginSlug,
      label: observation.pluginSlug,
      version: observation.installedVersion ?? "",
      skills: [],
      blocked: false,
      needsPreparation: true,
    }
    const location = observation.locations.find((item) =>
      item.agentTypes.includes(agentType)
    )
    const state = skill.agentStates.find((item) => item.agentType === agentType)
    const usable =
      location?.enabled && state?.effectiveEnabled && state.actualEnabled
    const structurallyUsable =
      !skill.conflict &&
      !skill.duplicate &&
      !skill.staleMarketRecord &&
      skill.pluginAvailable
    if (
      location &&
      usable &&
      structurallyUsable &&
      !group.skills.some((item) => item.id === skill.skillId)
    ) {
      group.skills.push({
        id: skill.skillId,
        name: observation.pluginComponentKey || skill.name,
        scope: skill.scope,
        layout: observation.layout,
        path: location.path,
        description: observation.description,
        routing: observation.routing,
        routing_status: observation.routingStatus,
        routing_error: observation.routingError,
        enabled: location.enabled,
        copy_mode: Boolean(location.projectionSource),
        read_only: observation.readOnly,
        official: observation.ownership === "market",
        market_managed: Boolean(observation.marketSkillId),
      })
    }
    group.blocked = group.skills.length === 0
    groups.set(id, group)
  }
  return [...groups.values()]
}

export function enrichPluginChoices(
  local: ComposerPluginChoice[],
  catalog: SkillMarketV2Item[]
): ComposerPluginChoice[] {
  const choices = new Map(local.map((item) => [item.id, { ...item }]))
  for (const item of catalog) {
    if (item.packageType !== "plugin" || !item.installedVersion) continue
    const manifest = item.currentVersion.plugin
    const hasSkills =
      manifest?.components.some((part) => part.type === "skill") ?? false
    const hasExecution =
      item.currentVersion.version !== item.installedVersion ||
      (manifest?.components.some((part) =>
        ["connector", "runtime", "capability", "app"].includes(part.type)
      ) ??
        true)
    const hasApps =
      manifest?.components.some((part) => part.type === "app") ?? false
    const existing = choices.get(item.id)
    if (existing) {
      if (existing.slug === item.slug) existing.label = item.displayName
      existing.needsPreparation = hasExecution
      continue
    }
    if (hasSkills || hasApps || !hasExecution) continue
    choices.set(item.id, {
      id: item.id,
      slug: item.slug,
      label: item.displayName,
      version: item.installedVersion,
      skills: [],
      blocked: false,
      needsPreparation: hasExecution,
    })
  }
  return [...choices.values()].sort((left, right) =>
    left.label.localeCompare(right.label)
  )
}

export async function loadComposerPluginCatalog() {
  const result = await getSkillMarketSource().list({
    view: "installed",
    publisher: "all",
    distribution: "all",
    compatibility: "all",
    category: null,
    q: "",
    sort: "updated",
    cursor: null,
    limit: Number.MAX_SAFE_INTEGER,
  })
  return result.items.filter((item) => item.packageType === "plugin")
}
