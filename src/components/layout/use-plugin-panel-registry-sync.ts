import { useEffect } from "react"
import { usePluginPanelRegistry } from "@/stores/plugin-panel-registry"
import {
  onDraftPluginContextChanged,
  persistedPluginConversation,
} from "@/lib/plugin-draft-context"
import type { PluginPanelTab } from "./use-plugin-panel-tabs"

export function usePluginPanelRegistrySync(tabs: PluginPanelTab[]) {
  useEffect(() => {
    const publish = () =>
      usePluginPanelRegistry.getState().setPanels(
        tabs.flatMap((tab) =>
          tab.kind === "mcp-app"
            ? [
                {
                  id: tab.id,
                  label: tab.label,
                  pluginSlug: tab.pluginSlug,
                  appKey: tab.appKey,
                  conversationId:
                    persistedPluginConversation(tab.conversationId) ??
                    tab.conversationId,
                  resourceUri: tab.response.launch.resourceUri,
                  pluginVersion: tab.response.launch.pluginVersion,
                },
              ]
            : []
        )
      )
    publish()
    return onDraftPluginContextChanged(publish)
  }, [tabs])
  useEffect(() => () => usePluginPanelRegistry.getState().clear(), [])
}
