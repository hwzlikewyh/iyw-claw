const TEARDOWN_TIMEOUT_MS = 15_000
type CloseHandler = () => Promise<boolean>
const handlers = new Map<string, CloseHandler>()
let requestSequence = 0

export function registerPluginAppTeardown(
  instanceId: string,
  handler: CloseHandler
) {
  handlers.set(instanceId, handler)
  return () => {
    if (handlers.get(instanceId) === handler) handlers.delete(instanceId)
  }
}

export function requestPluginAppTeardown(instanceId: string) {
  return handlers.get(instanceId)?.() ?? Promise.resolve(true)
}

export function createPluginAppTeardown(
  send: (message: Record<string, unknown>) => boolean
) {
  let pending: {
    id: string
    promise: Promise<boolean>
    finish: (success: boolean) => void
  } | null = null
  return {
    request() {
      if (pending) return pending.promise
      const id = `teardown-${Date.now()}-${++requestSequence}`
      let settle!: (success: boolean) => void
      const promise = new Promise<boolean>((resolve) => {
        settle = resolve
      })
      const timer = setTimeout(() => finish(false), TEARDOWN_TIMEOUT_MS)
      const finish = (success: boolean) => {
        clearTimeout(timer)
        if (pending?.id === id) pending = null
        settle(success)
      }
      pending = { id, promise, finish }
      try {
        if (
          !send({
            jsonrpc: "2.0",
            id,
            method: "ui/resource-teardown",
            params: { reason: "user_close" },
          })
        )
          finish(false)
      } catch {
        finish(false)
      }
      return promise
    },
    accept(value: unknown) {
      if (!pending || !value || typeof value !== "object") return false
      const message = value as Record<string, unknown>
      if (message.jsonrpc !== "2.0" || message.id !== pending.id) return false
      pending.finish("result" in message && !("error" in message))
      return true
    },
    dispose: () => pending?.finish(false),
  }
}
