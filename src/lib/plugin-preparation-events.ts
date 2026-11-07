interface PreparationEvent {
  pluginId: string
  conversationId: number | null
  canceled?: boolean
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
