import { ALL_AGENT_TYPES, type AcpAgentInfo, type AgentType } from "@/lib/types"

export const AGENT_SDK_ALIASES = {
  codex: "星河",
  open_code: "云舟",
  code_buddy: "青岚",
  claude_code: "远山",
  gemini: "流光",
  cline: "逐风",
  kimi_code: "月白",
  pi: "墨川",
  grok: "知微",
} as const satisfies Partial<Record<AgentType, string>>

const VISIBLE_AGENT_TYPES = new Set<AgentType>(ALL_AGENT_TYPES)

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
  return agents
    .filter((agent) => VISIBLE_AGENT_TYPES.has(agent.agent_type))
    .map((agent) => {
      const alias =
        AGENT_SDK_ALIASES[agent.agent_type as keyof typeof AGENT_SDK_ALIASES]
      if (!alias) return agent
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
