import type { Transport } from "@/lib/transport"
import type {
  AgentSkillItem,
  AgentSkillsListResult,
  AgentType,
  SkillInventorySnapshot,
} from "@/lib/types"
import type { SkillMarketV2Item } from "@/lib/skill-market"
import { getSkillMarketSource } from "@/lib/skill-market-source"
import { readInstalledPlugins } from "@/lib/plugin-panel"

export interface ComposerPluginChoice {
  id: string
  slug: string
  label: string
  version: string
  skills: AgentSkillItem[]
  blocked: boolean
}

export async function loadComposerPluginInventory(
  transport: Transport,
  context: { agentType: AgentType; workspacePath: string | null }
) {
  const apps = await readInstalledPlugins(transport)
  const [inventory, agent] = await Promise.all([
    transport.call<SkillInventorySnapshot>("skill_inventory_list", {
      workspacePath: context.workspacePath,
    }),
    transport.call<AgentSkillsListResult>("acp_list_agent_skills", context),
  ])
  return { inventory, agent, apps }
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
    }
    const entry = data.agent.skills.find(
      (item) => item.id === skill.skillId && item.scope === skill.scope
    )
    const state = skill.agentStates.find((item) => item.agentType === agentType)
    const usable =
      entry?.enabled && state?.effectiveEnabled && state.actualEnabled
    group.blocked ||= Boolean(
      !usable ||
      skill.conflict ||
      skill.duplicate ||
      skill.staleMarketRecord ||
      !skill.pluginAvailable
    )
    if (entry && !group.skills.some((item) => item.id === entry.id)) {
      group.skills.push({
        ...entry,
        name: observation.pluginComponentKey || entry.name,
      })
    }
    groups.set(id, group)
  }
  for (const app of data.apps) {
    if (groups.has(app.pluginId)) continue
    groups.set(app.pluginId, {
      id: app.pluginId,
      slug: app.pluginSlug,
      label: app.label,
      version: app.version,
      skills: [],
      blocked: app.state === "unavailable",
    })
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
    const existing = choices.get(item.id)
    if (existing) {
      if (existing.slug === item.slug) existing.label = item.displayName
      continue
    }
  }
  return [...choices.values()].sort((left, right) =>
    left.label.localeCompare(right.label)
  )
}

// 市场只补充显示名称；列表和版本以本地安装库存为准。
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
