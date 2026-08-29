import type { AgentType } from "@/lib/types"

const MANAGED_GATEWAY_AGENTS = new Set<AgentType>([
  "claude_code",
  "codex",
  "open_code",
  "open_claw",
  "cline",
  "hermes",
  "code_buddy",
  "kimi_code",
  "pi",
  "grok",
])

const DEFAULT_MODEL_REAPPLY_TIMEOUT_MS = 60_000
const SLOW_MODEL_REAPPLY_TIMEOUT_MS = 120_000

export function hasManagedGatewayModelProjection(
  agentType: AgentType
): boolean {
  return MANAGED_GATEWAY_AGENTS.has(agentType)
}

export function modelReapplyTimeoutMs(agentType: AgentType): number {
  return agentType === "pi"
    ? SLOW_MODEL_REAPPLY_TIMEOUT_MS
    : DEFAULT_MODEL_REAPPLY_TIMEOUT_MS
}
