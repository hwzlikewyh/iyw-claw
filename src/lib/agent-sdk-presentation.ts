import {
  AGENT_LABELS,
  isAgentType,
  type AcpAgentInfo,
  type AgentType,
} from "@/lib/types"
import { getAgentLabel, refreshCustomAgentNames } from "@/lib/custom-agents"

export const AGENT_SDK_ALIASES = AGENT_LABELS

export function getAgentDisplayName(agentType: AgentType): string {
  return getAgentLabel(agentType)
}

const BRAND_TEXT_REPLACEMENTS: ReadonlyArray<readonly [RegExp, string]> = [
  [/Codex CLI/g, "星河"],
  [/Codex/g, "星河"],
  [/OpenCode/g, "云舟"],
  [/CodeBuddy/g, "青岚"],
  [/Claude Code/g, "远山"],
  [/Gemini CLI/g, "流光"],
  [/Gemini/g, "流光"],
  [/Cline/g, "逐风"],
  [/Kimi Code/g, "月白"],
  [/\bPi\b/g, "墨川"],
  [/Grok Build/g, "知微"],
  [/\bGrok\b/g, "知微"],
]

export function isAgentSdkConfigurationVisible(agentType: AgentType): boolean {
  void agentType
  return false
}

export function maskAgentSdkBrandText(text: string): string {
  return BRAND_TEXT_REPLACEMENTS.reduce(
    (result, [pattern, replacement]) => result.replace(pattern, replacement),
    text
  )
}

export function maskAgentSdkTranslator<TArgs extends unknown[]>(
  translate: (...args: TArgs) => string
): (...args: TArgs) => string {
  return (...args) => maskAgentSdkBrandText(translate(...args))
}

export function presentAgentSdkAgents(
  agents: AcpAgentInfo[],
  describeAlias: (name: string) => string
): AcpAgentInfo[] {
  refreshCustomAgentNames(agents)
  return agents
    .filter((agent) => isAgentType(agent.agent_type))
    .map((agent) => {
      const alias = getAgentDisplayName(agent.agent_type)
      return {
        ...agent,
        name: alias,
        description: describeAlias(alias),
      }
    })
    .sort(
      (left, right) =>
        left.sort_order - right.sort_order ||
        left.name.localeCompare(right.name)
    )
}
