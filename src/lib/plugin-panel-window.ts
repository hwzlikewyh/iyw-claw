import { getActiveRemoteConnectionId, getShellTransport } from "./transport"
import type { PluginPanelChoice } from "./plugin-panel"
import { createPluginWindowRelay } from "./plugin-window-relay"

export async function openPluginPanelWindow(
  app: PluginPanelChoice,
  conversationId: number
) {
  const relay = createPluginWindowRelay(conversationId)
  try {
    return await getShellTransport().call<string>(
      "open_plugin_panel_window",
      {
        request: {
          conversationId,
          pluginSlug: app.pluginSlug,
          appKey: app.appKey,
          expectedVersion: app.version,
          label: app.label,
          relayId: relay.id,
        },
        remoteConnectionId: getActiveRemoteConnectionId(),
      },
      { timeoutMs: 800_000 }
    )
  } catch (error) {
    relay.dispose()
    throw error
  }
}

export function reportPluginWindowReady(error: string | null = null) {
  return getShellTransport().call("plugin_panel_window_ready", { error })
}
