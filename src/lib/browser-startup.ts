import { browserApi } from "@/lib/browser-api"
import type {
  BrowserErrorEnvelope,
  BrowserStateSnapshot,
} from "@/lib/browser-types"

const STARTUP_WAIT_MS = 120_000
let pendingStart: Promise<BrowserStateSnapshot> | null = null

export async function startBrowserRuntime(): Promise<BrowserStateSnapshot> {
  // 前端超时不取消 IPC；重试继续等待同一请求，避免排队启动多个运行时。
  pendingStart ??= browserApi.start().finally(() => {
    pendingStart = null
  })
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      pendingStart,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          const error: BrowserErrorEnvelope = {
            code: "BROWSER_RUNTIME_START_TIMEOUT",
            message:
              "Browser startup did not finish within 120 seconds. The runtime may still be preparing; retry checks the pending request.",
            retryable: true,
            effectMayHaveOccurred: true,
          }
          console.warn("[Browser] startup response timed out", {
            code: error.code,
            timeoutMs: STARTUP_WAIT_MS,
          })
          reject(error)
        }, STARTUP_WAIT_MS)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

export function normalizeBrowserError(cause: unknown): BrowserErrorEnvelope {
  const value = cause as Partial<BrowserErrorEnvelope> | null
  return {
    code: value?.code ?? "BROWSER_INTERNAL",
    message: value?.message ?? String(cause),
    retryable: value?.retryable ?? false,
    effectMayHaveOccurred: value?.effectMayHaveOccurred ?? false,
  }
}
