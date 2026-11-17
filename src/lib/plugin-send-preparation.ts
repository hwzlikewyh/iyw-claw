import { useTabStore } from "@/stores/tab-store"
import {
  selectComposerPlugin,
  usePluginComposerContext,
} from "./plugin-composer-context"
import { preparePluginSkills, preparePluginUse } from "./plugin-use"
import { readPluginPanels } from "./plugin-panel"
import type { PluginPanelChoice } from "./plugin-panel"
import { requestPluginApplicationOpen } from "./plugin-preparation-events"
import { getTransport } from "./transport"

// 只有首次发送创建了真实对话后，才准备草稿中选择的插件。
export async function prepareDeferredPluginForSend(
  tabId: string,
  selected: PluginPanelChoice | undefined
) {
  if (!selected) return
  const transport = getTransport()
  const conversationId = useTabStore
    .getState()
    .tabs.find((tab) => tab.id === tabId)?.conversationId
  const current = () =>
    getTransport() === transport &&
    usePluginComposerContext.getState().entries[tabId]?.selected === selected &&
    useTabStore
      .getState()
      .tabs.some(
        (tab) => tab.id === tabId && tab.conversationId === conversationId
      )
  const context = { tabId, transport, current }
  const plugin = {
    id: selected.pluginId,
    slug: selected.pluginSlug,
    version: selected.version,
    label: selected.label,
    skills: [],
    blocked: false,
  }
  const ready = await preparePluginUse(plugin, context)
  if (!ready || !current()) return
  if (!["unchecked", "legacy"].includes(ready.view.state))
    throw Error("插件尚未准备好，请在能力市场检查配置后重新发送")
  await preparePluginSkills(plugin, context)
  if (!current()) return
  const apps = await readPluginPanels(ready.conversationId, transport)
  if (!current()) return
  if (usePluginComposerContext.getState().entries[tabId]?.selected !== selected)
    return
  const app = apps.find(
    (item) => item.pluginId === selected.pluginId && item.state === "available"
  )
  selectComposerPlugin(tabId, app ?? selected)
  if (app && useTabStore.getState().activeTabId === tabId)
    requestPluginApplicationOpen(app)
}
