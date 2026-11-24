const TEARDOWN_TIMEOUT_MS = 15_000
const METHOD_NOT_FOUND = -32601
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
  let pending: ReturnType<typeof createRequest> | null = null
  return {
    request() {
      if (pending) return pending.promise
      const request = createRequest(() => {
        pending = null
      })
      pending = request
      try {
        if (!send(request.message)) request.finish(false)
      } catch {
        request.finish(false)
      }
      return request.promise
    },
    accept(value: unknown) {
      if (!pending || !value || typeof value !== "object") return false
      const message = value as Record<string, unknown>
      if (message.jsonrpc !== "2.0" || message.id !== pending.message.id)
        return false
      pending.finish(teardownSucceeded(message))
      return true
    },
    dispose: () => pending?.finish(false),
  }
}

function createRequest(clear: () => void) {
  const id = `teardown-${Date.now()}-${++requestSequence}`
  let settle!: (success: boolean) => void
  const promise = new Promise<boolean>((resolve) => {
    settle = resolve
  })
  const timer = setTimeout(() => {
    console.warn("Plugin app teardown timed out")
    finish(false)
  }, TEARDOWN_TIMEOUT_MS)
  const finish = (success: boolean) => {
    clearTimeout(timer)
    clear()
    settle(success)
  }
  return {
    message: {
      jsonrpc: "2.0",
      id,
      method: "ui/resource-teardown",
      params: { reason: "user_close" },
    },
    promise,
    finish,
  }
}

function teardownSucceeded(message: Record<string, unknown>) {
  const error = message.error as { code?: unknown } | null | undefined
  const unsupported = !("result" in message) && error?.code === METHOD_NOT_FOUND
  const success = ("result" in message && !("error" in message)) || unsupported
  if (!success)
    console.warn("Plugin app teardown rejected", {
      code: typeof error?.code === "number" ? error.code : undefined,
    })
  return success
}
