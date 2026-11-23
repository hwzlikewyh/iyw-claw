import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import { getTransport } from "@/lib/transport"

const RENEW_INTERVAL_MS = 30_000
const RENEW_RETRY_DELAY_MS = 2_000
const MAX_RENEW_FAILURES = 2
const MAX_RECOVERIES = 2
const RENEW_TIMEOUT_MS = 10_000

type FailurePhase = "open" | "renew"
type PreviewConnection = { port: number; cap: string }
type PreviewTarget = { rootPath: string; path: string }
type PreviewHandlers = {
  onReady: (connection: PreviewConnection) => void
  onRecovering: () => void
  onError: (error: unknown, phase: FailurePhase) => void
}

// 会话和定时器一起回收；旧请求只允许释放自己的会话。
class OfficePreviewSession {
  private readonly transport = getTransport()
  private id = ""
  private disposed = false
  private ready = false
  private renewFailures = 0
  private recoveries = 0
  private timer: ReturnType<typeof setTimeout> | undefined

  constructor(
    private readonly target: PreviewTarget,
    private readonly handlers: PreviewHandlers
  ) {}

  start() {
    this.id = crypto.randomUUID()
    this.ready = false
    this.renewFailures = 0
    // 大文档导入期间也续租，避免后台尚未返回就被租约清理。
    this.scheduleRenew(RENEW_INTERVAL_MS)
    void this.open(this.id)
  }

  close() {
    if (this.disposed) return
    this.disposed = true
    clearTimeout(this.timer)
    this.release(this.id)
  }

  private isCurrent(id: string) {
    return !this.disposed && this.id === id
  }

  private release(id: string) {
    void this.transport.call("close_office_preview", { id }).catch((error) => {
      console.warn("[office-preview] session release failed", {
        reason: toErrorMessage(error),
      })
    })
  }

  private async open(id: string) {
    try {
      const connection = await this.transport.call<PreviewConnection>(
        "open_office_preview",
        { id, ...this.target }
      )
      if (!this.isCurrent(id)) {
        this.release(id)
        return
      }
      this.ready = true
      if (this.recoveries > 0) {
        console.info("[office-preview] preview session restored", {
          attempts: this.recoveries,
        })
      }
      this.handlers.onReady(connection)
    } catch (error) {
      if (this.isCurrent(id)) {
        this.fail(error, this.recoveries > 0 ? "renew" : "open")
      }
    }
  }

  private scheduleRenew(delay: number) {
    this.timer = setTimeout(() => void this.renew(this.id), delay)
  }

  private async renew(id: string) {
    if (!this.isCurrent(id)) return
    let delay = RENEW_INTERVAL_MS
    try {
      await this.transport.call(
        "renew_office_preview",
        { id },
        { timeoutMs: RENEW_TIMEOUT_MS }
      )
      if (!this.isCurrent(id)) return
      this.renewFailures = 0
      if (this.ready) this.recoveries = 0
    } catch (error) {
      if (!this.isCurrent(id)) return
      this.renewFailures += 1
      if (this.shouldRecover(error)) {
        this.recover(error)
        return
      }
      if (this.renewFailures === 1) {
        console.warn("[office-preview] renewal failed; retrying", {
          ready: this.ready,
          reason: toErrorMessage(error),
        })
      }
      delay = RENEW_RETRY_DELAY_MS
    }
    // 上一次请求结束后才安排下一次，避免慢请求造成并发续租。
    if (this.isCurrent(id)) this.scheduleRenew(delay)
  }

  private shouldRecover(error: unknown) {
    return (
      (this.ready && extractAppCommandError(error)?.code === "not_found") ||
      this.renewFailures >= MAX_RENEW_FAILURES
    )
  }

  private recover(error: unknown) {
    if (this.recoveries >= MAX_RECOVERIES) {
      this.fail(error, "renew")
      return
    }
    this.recoveries += 1
    console.warn("[office-preview] reopening preview session", {
      attempt: this.recoveries,
      reason: toErrorMessage(error),
    })
    this.release(this.id)
    this.handlers.onRecovering()
    this.start()
  }

  private fail(error: unknown, phase: FailurePhase) {
    console.warn("[office-preview] preview session failed", {
      phase,
      reason: toErrorMessage(error),
    })
    this.close()
    this.handlers.onError(error, phase)
  }
}

export function acquireOfficePreview(
  target: PreviewTarget,
  handlers: PreviewHandlers
) {
  const session = new OfficePreviewSession(target, handlers)
  session.start()
  return () => session.close()
}
