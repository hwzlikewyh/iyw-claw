import { getTransport } from "@/lib/transport"
import type { PluginAppOpenResponse } from "@/lib/api"
import { draftPluginTab } from "./plugin-draft-context"
import { useTabStore } from "@/stores/tab-store"

export const OPENFILM_STUDIO_APP_KEY = "iyw-openfilm-studio"
const PLUGIN_APP_REQUEST_TIMEOUT_MS = 660_000

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

export function isPluginPanelVisibleForConversation(
  tab: {
    kind: "mcp-app" | "studio-url"
    conversationId: number | null
  },
  conversationId: number | null
) {
  return (
    (tab.conversationId != null &&
      tab.conversationId < 0 &&
      draftPluginTab(tab.conversationId) ===
        useTabStore.getState().activeTabId) ||
    (tab.conversationId != null && tab.conversationId === conversationId)
  )
}

export const readPluginPanels = (
  conversationId: number | null,
  transport = getTransport()
) =>
  transport.call<PluginPanelChoice[]>("plugin_app_catalog", {
    request: { conversationId },
  })

// 包含没有独立页面的技能、工具插件；应用启动仍只使用 readPluginPanels。
export const readInstalledPlugins = (transport = getTransport()) =>
  transport.call<PluginPanelChoice[]>("plugin_app_catalog", {
    request: { conversationId: null, includePlugins: true },
  })

export const openOpenfilmStudio = (
  conversationId: number | null,
  app: PluginPanelChoice,
  transport = getTransport()
) =>
  transport.call<{ url: string }>(
    "plugin_openfilm_studio",
    {
      request: {
        conversationId,
        pluginSlug: app.pluginSlug,
        expectedVersion: app.version,
      },
    },
    { timeoutMs: PLUGIN_APP_REQUEST_TIMEOUT_MS }
  )
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
    { timeoutMs: PLUGIN_APP_REQUEST_TIMEOUT_MS }
  )
