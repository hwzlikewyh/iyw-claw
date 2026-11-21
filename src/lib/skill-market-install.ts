import { getTransport } from "@/lib/transport"
import type { AgentType } from "@/lib/types"
import { randomUUID } from "@/lib/utils"

export interface MarketInstallProgress {
  phase: "downloading" | "verifying" | "installing"
  receivedBytes: number
  totalBytes: number | null
}

interface InstallObserver {
  signal: AbortSignal
  onProgress: (progress: MarketInstallProgress) => void
}

const INSTALL_TIMEOUT_MS = 30 * 60_000

export async function installMarketPackage(
  input: { id: string; version: string; agentTypes: AgentType[] },
  observer?: InstallObserver
) {
  const transport = getTransport()
  const taskId = observer ? randomUUID() : undefined
  let unsubscribe: (() => void) | undefined
  const stop = () => {
    unsubscribe?.()
    unsubscribe = undefined
  }
  try {
    if (observer) {
      try {
        unsubscribe = await transport.subscribe<MarketInstallProgress>(
          `skill-market://install-progress/${taskId}`,
          (value) => {
            if (!observer.signal.aborted) observer.onProgress(value)
          }
        )
      } catch (error) {
        console.warn(
          "[SkillMarket] Could not subscribe to install progress",
          error
        )
      }
      if (observer.signal.aborted) stop()
      else observer.signal.addEventListener("abort", stop, { once: true })
    }
    await transport.call<void>(
      "skill_market_install",
      { ...input, taskId },
      {
        timeoutMs: INSTALL_TIMEOUT_MS,
      }
    )
  } finally {
    observer?.signal.removeEventListener("abort", stop)
    stop()
  }
}
