import { isCustomAgentType } from "@/lib/types"
import type {
  AcpAgentInfo,
  AgentType,
  BuiltinAgentType,
  CheckStatus,
} from "@/lib/types"

type ProfiledBuiltinAgentType = Exclude<BuiltinAgentType, "cursor" | "deepseek">

type AgentProfileMessageKey =
  | `profiles.${ProfiledBuiltinAgentType}.description`
  | `profiles.${ProfiledBuiltinAgentType}.strengths.primary`
  | `profiles.${ProfiledBuiltinAgentType}.strengths.secondary`
  | `profiles.${ProfiledBuiltinAgentType}.strengths.tertiary`

interface AgentProfileMessageKeys {
  description: AgentProfileMessageKey
  strengths: readonly [
    AgentProfileMessageKey,
    AgentProfileMessageKey,
    AgentProfileMessageKey,
  ]
}

function profile(agentType: ProfiledBuiltinAgentType): AgentProfileMessageKeys {
  return {
    description: `profiles.${agentType}.description`,
    strengths: [
      `profiles.${agentType}.strengths.primary`,
      `profiles.${agentType}.strengths.secondary`,
      `profiles.${agentType}.strengths.tertiary`,
    ],
  }
}

export const AGENT_PROFILE_MESSAGE_KEYS: Partial<
  Record<BuiltinAgentType, AgentProfileMessageKeys>
> = {
  claude_code: profile("claude_code"),
  codex: profile("codex"),
  open_code: profile("open_code"),
  gemini: profile("gemini"),
  open_claw: profile("open_claw"),
  cline: profile("cline"),
  hermes: profile("hermes"),
  code_buddy: profile("code_buddy"),
  kimi_code: profile("kimi_code"),
  pi: profile("pi"),
  grok: profile("grok"),
}

export function getAgentProfileMessageKeys(
  agentType: AgentType
): AgentProfileMessageKeys | null {
  if (isCustomAgentType(agentType)) return null
  return AGENT_PROFILE_MESSAGE_KEYS[agentType] ?? null
}

export type AgentVersionState =
  | "notInstalled"
  | "upgradeAvailable"
  | "latest"
  | "unknown"
  | "unsupported"

function comparableVersion(value: string | null): number[] | null {
  if (!value || !/\d/.test(value) || !value.includes(".")) return null

  return value
    .trim()
    .replace(/^[^\d]*/, "")
    .split(".")
    .map((part) => Number.parseInt(part, 10) || 0)
}

function compareVersions(left: number[], right: number[]): number {
  const length = Math.max(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0)
    if (difference !== 0) return difference
  }
  return 0
}

export function getAgentVersionState(agent: AcpAgentInfo): AgentVersionState {
  if (
    !agent.available &&
    agent.distribution_type === "binary" &&
    !agent.installed_version
  ) {
    return "unsupported"
  }
  if (!agent.installed_version) return "notInstalled"

  const installed = comparableVersion(agent.installed_version)
  const registry = comparableVersion(agent.registry_version)
  if (!installed || !registry) return "unknown"

  return compareVersions(installed, registry) < 0
    ? "upgradeAvailable"
    : "latest"
}

interface RuntimeCheck {
  check_id: string
  status: CheckStatus
}

export function needsManagedRuntimePreparation(
  agent: AcpAgentInfo,
  checks: readonly RuntimeCheck[]
): boolean {
  return (
    agent.distribution_type === "uvx" &&
    checks.some(
      (check) => check.check_id === "uv_available" && check.status !== "pass"
    )
  )
}
