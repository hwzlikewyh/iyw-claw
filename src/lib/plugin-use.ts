import { useTabStore } from "@/stores/tab-store"
import { prepareConnectorConversation } from "./conversation-connectors"
import { preparePluginDraft } from "./plugin-composer-context"
import { getTransport } from "./transport"
import {
  changePluginPreparation,
  readPluginPreparation,
} from "./plugin-preparation"
import {
  inventoryPluginChoices,
  loadComposerPluginInventory,
  type ComposerPluginChoice,
} from "./composer-plugins"
import type {
  SkillActivationSetResult,
  AgentType,
  LogicalSkillInventoryItem,
} from "./types"

export interface PluginUseContext {
  tabId: string
  transport: ReturnType<typeof getTransport>
  current: () => boolean
}

export async function preparePluginUse(
  plugin: { id: string; version: string; label: string },
  context: PluginUseContext
) {
  const prepared = await prepareConnectorConversation(
    context.tabId,
    plugin.label
  )
  if (!context.current()) return null
  preparePluginDraft(context.tabId, prepared.id)
  prepared.bind()
  let view = await readPluginPreparation(
    { marketSkillId: plugin.id, conversationId: prepared.id },
    context.transport
  )
  if (!context.current()) return null
  if (view.installedVersion !== plugin.version)
    throw Error("插件版本已变化，请刷新后重试")
  if (
    ["permission_required", "disabled"].includes(view.state) &&
    view.contextToken
  ) {
    view = await changePluginPreparation(
      {
        marketSkillId: plugin.id,
        conversationId: prepared.id,
        contextToken: view.contextToken,
      },
      true,
      context.transport
    )
  }
  return context.current() ? { conversationId: prepared.id, view } : null
}

// 只启用用户本次选择的插件技能，使用既有库存校验与投影流程。
export async function preparePluginSkills(
  choice: ComposerPluginChoice,
  context: PluginUseContext
) {
  const tab = useTabStore
    .getState()
    .tabs.find((item) => item.id === context.tabId)
  if (!tab || !context.current()) return null
  const target = {
    agentType: tab.agentType,
    workspacePath: tab.workingDir ?? null,
  }
  let data = await loadComposerPluginInventory(context.transport, target)
  let changed = false
  for (const skill of data.inventory.skills) {
    if (!context.current()) return null
    if (
      !skill.observations.some(
        (item) =>
          item.marketSkillId === choice.id && item.pluginSlug === choice.slug
      )
    )
      continue
    const state = skill.agentStates.find(
      (item) => item.agentType === tab.agentType
    )
    if (state?.actualEnabled && state.effectiveEnabled) continue
    await enableSkill(skill, target, context.transport)
    changed = true
  }
  if (!context.current()) return null
  if (changed)
    data = await loadComposerPluginInventory(context.transport, target)
  const fresh = inventoryPluginChoices(data, tab.agentType).find(
    (item) => item.id === choice.id
  )
  if (fresh?.blocked) throw Error("插件暂时不可用，请检查安装状态后重试")
  return { ...choice, skills: fresh?.skills ?? [], blocked: false }
}

async function enableSkill(
  skill: LogicalSkillInventoryItem,
  target: { agentType: AgentType; workspacePath: string | null },
  transport: ReturnType<typeof getTransport>
) {
  const result = await transport.call<SkillActivationSetResult>(
    "skill_activation_set",
    {
      request: {
        skillId: skill.skillId,
        scope: skill.scope,
        ...target,
        enabled: true,
        expectedRevision: null,
        syncMode: null,
      },
    }
  )
  if (result.error) throw Error(result.error)
}
