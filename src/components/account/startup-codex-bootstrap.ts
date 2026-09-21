import {
  acpDetectAgentLocalVersion,
  acpListAgents,
  acpPrepareNpxAgent,
} from "@/lib/api"
import { isLocalDesktop } from "@/lib/platform"
import { prepareStartupRuntime } from "@/lib/startup-runtime"
import type { AcpAgentInfo, BootstrapInitStatusReport } from "@/lib/types"

export type BootstrapStep = "runtime" | "registry" | "detect" | "install"

interface BootstrapMessages {
  componentPending: string
  repairCore: string
}

interface BootstrapContext {
  repair: boolean
  runtimeTaskId: string
  installTaskId: string
  signal: AbortSignal
  messages: BootstrapMessages
  onStep: (step: BootstrapStep) => void
  onStatus: (report: BootstrapInitStatusReport) => void
  onInstalling: () => void
  refreshAgents: () => Promise<unknown>
}

export async function executeCodexBootstrap(
  context: BootstrapContext
): Promise<void> {
  context.onStep("runtime")
  const report = await prepareStartupRuntime({
    repair: context.repair,
    taskId: context.runtimeTaskId,
    signal: context.signal,
    onStatus: context.onStatus,
  })
  assertRequiredComponents(report, context.messages.componentPending)
  // 桌面内置 Agent 随应用发布，不依赖另装的 Codex CLI 或在线注册表。
  if (isLocalDesktop()) return
  context.onStep("registry")
  const codex = await loadCodexAgent()
  if (!codex) {
    await context.refreshAgents()
    return
  }
  await ensureCodexInstalled(context, codex)
  await context.refreshAgents()
}

async function loadCodexAgent(): Promise<AcpAgentInfo | undefined> {
  const agents = await acpListAgents().catch((error) => {
    console.warn(
      "[StartupCodexGate] Agent registry unavailable; continuing fail-closed:",
      error
    )
    return []
  })
  return agents.find((agent) => agent.agent_type === "codex")
}

function assertRequiredComponents(
  report: BootstrapInitStatusReport,
  pendingMessage: string
): void {
  const components = new Map(
    report.components.map((component) => [component.componentId, component])
  )
  const environmentError = components.get("environment")?.lastError
  if (environmentError) throw new Error(environmentError)
  const required = isLocalDesktop()
    ? [...components.keys(), "builtin-agent"]
    : ["node", "git", "uv"]
  const failures = required.flatMap((componentId) => {
    const component = components.get(componentId)
    if (!component) return [`${componentId}: ${pendingMessage}`]
    if (!component.installed || !component.active) {
      return [`${componentId}: ${component.lastError ?? component.phase}`]
    }
    return []
  })
  if (failures.length > 0) throw new Error(failures.join("\n"))
}

async function ensureCodexInstalled(
  context: BootstrapContext,
  codex: AcpAgentInfo
): Promise<void> {
  context.onStep("detect")
  const installed =
    codex.installed_version ?? (await acpDetectAgentLocalVersion("codex"))
  if (installed) return
  if (isLocalDesktop()) throw new Error(context.messages.repairCore)
  context.onInstalling()
  context.onStep("install")
  await acpPrepareNpxAgent(
    "codex",
    codex.registry_version,
    context.installTaskId,
    false
  )
}
