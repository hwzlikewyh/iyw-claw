import type { PluginPanelChoice } from "./plugin-panel"

interface PreparationEvent {
  pluginId: string
  conversationId: number | null
  canceled?: boolean
}

const applicationListeners = new Set<(app: PluginPanelChoice) => void>()

export function requestPluginApplicationOpen(app: PluginPanelChoice) {
  for (const listener of applicationListeners) listener(app)
  return applicationListeners.size > 0
}

export function onPluginApplicationOpen(
  listener: (app: PluginPanelChoice) => void
) {
  applicationListeners.add(listener)
  return () => {
    applicationListeners.delete(listener)
  }
}

const listeners = new Set<(event: PreparationEvent) => void>()

// 仅通知目录重读；权限与就绪状态仍以当前后端返回为准。
export function notifyPluginPreparation(event: PreparationEvent) {
  for (const listener of listeners) listener(event)
}

export function onPluginPreparation(
  listener: (event: PreparationEvent) => void
) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}
