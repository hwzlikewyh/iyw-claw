import { bootstrapInitialize } from "@/lib/api"
import type { BootstrapInitStatusReport } from "@/lib/types"

const WRITER_RETRY_INTERVAL_MS = 2_000
const WRITER_WAIT_TIMEOUT_MS = 120_000

export async function prepareStartupRuntime(options: {
  repair?: boolean
  taskId: string
  signal: AbortSignal
  onStatus: (report: BootstrapInitStatusReport) => void
}) {
  const deadline = Date.now() + WRITER_WAIT_TIMEOUT_MS
  let repair = options.repair ?? false
  while (true) {
    options.signal.throwIfAborted()
    const report = await bootstrapInitialize({ repair, taskId: options.taskId })
    repair = false
    options.signal.throwIfAborted()
    options.onStatus(report)
    if (!report.writerBusy) return report
    if (Date.now() >= deadline) {
      throw new Error("另一个窗口仍在准备运行环境，请稍后重试。")
    }
    await waitForWriter(options.signal)
  }
}

function waitForWriter(signal: AbortSignal): Promise<void> {
  signal.throwIfAborted()
  return new Promise((resolve, reject) => {
    const onAbort = () => {
      clearTimeout(timer)
      reject(signal.reason)
    }
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", onAbort)
      resolve()
    }, WRITER_RETRY_INTERVAL_MS)
    signal.addEventListener("abort", onAbort, { once: true })
  })
}
