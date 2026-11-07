import { getTransport } from "@/lib/transport"
import type { PluginAppOpenResponse } from "@/lib/api"

export interface PluginPanelUpdate {
  cursor: string
  arguments: unknown
  result: unknown
}

export interface PluginPanelSyncState {
  update?: PluginPanelUpdate
  error?: boolean
  forbidden?: boolean
}

export interface PluginPanelChoice {
  pluginId: string
  pluginSlug: string
  version: string
  appKey: string
  label: string
  state:
    | "available"
    | "context_required"
    | "permission_required"
    | "dependency_required"
    | "disabled"
    | "unavailable"
}
export const readPluginPanels = (
  conversationId: number | null,
  transport = getTransport()
) =>
  transport.call<PluginPanelChoice[]>("plugin_app_catalog", {
    request: { conversationId },
  })
export const launchPluginPanel = (
  conversationId: number,
  app: PluginPanelChoice,
  transport = getTransport()
) =>
  transport.call<PluginAppOpenResponse>(
    "plugin_app_launch",
    {
      request: {
        conversationId,
        pluginSlug: app.pluginSlug,
        appKey: app.appKey,
        expectedVersion: app.version,
      },
    },
    { timeoutMs: 750_000 }
  )
