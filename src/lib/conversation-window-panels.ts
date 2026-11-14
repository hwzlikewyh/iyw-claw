import { requestPluginAppTeardown } from "./plugin-app-teardown"
import type { PluginPanelChoice } from "./plugin-panel"

export interface ConversationWindowPanels {
  isOpen: boolean
  width: number
  activeTab: string
  apps: Array<{ id: string; pluginSlug: string; appKey: string }>
  selected?: PluginPanelChoice
  preparedConversationId?: number
}

let capture:
  | ((conversationId: number | null) => ConversationWindowPanels)
  | null = null
let incoming: ConversationWindowPanels | null = null

export function registerConversationPanels(
  reader: NonNullable<typeof capture>
) {
  capture = reader
  return () => {
    if (capture === reader) capture = null
  }
}

export function captureConversationPanels(conversationId: number | null) {
  return capture?.(conversationId) ?? null
}

export async function saveConversationPanels(
  panels: ConversationWindowPanels | null
) {
  for (const app of panels?.apps ?? []) {
    if (!(await requestPluginAppTeardown(app.id)))
      throw Error("插件尚未完成保存，当前会话已保留")
  }
}

export function seedConversationPanels(
  panels: ConversationWindowPanels | null
) {
  incoming = panels
}

export function peekConversationPanels() {
  return incoming
}
export function takeConversationPanels() {
  const panels = incoming
  incoming = null
  return panels
}
