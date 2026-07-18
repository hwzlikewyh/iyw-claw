import type {
  AgentOptionsSnapshot,
  AgentType,
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"
import {
  localizeSessionConfigOption,
  type SessionConfigTranslator,
} from "@/lib/session-config-localization"

const MANAGED_MODELS: SessionConfigSelectOptionInfo[] = [
  { value: "deepseek-v4-pro", name: "DeepSeek V4 Pro", description: null },
  {
    value: "doubao-seed-2-1-pro-260628",
    name: "Doubao Seed 2.1 Pro",
    description: null,
  },
  {
    value: "deepseek-v4-flash",
    name: "DeepSeek V4 Flash",
    description: null,
  },
]

const REASONING_OPTIONS: SessionConfigSelectOptionInfo[] = [
  { value: "low", name: "Low", description: "Quick, fast responses" },
  {
    value: "medium",
    name: "Medium",
    description: "Balanced speed and quality",
  },
  {
    value: "high",
    name: "High",
    description: "Extensive reasoning for high quality",
  },
  {
    value: "xhigh",
    name: "Max",
    description: "Maximum reasoning for the most complex tasks",
  },
]

const CODEX_MODE: SessionConfigOptionInfo = {
  id: "mode",
  name: "Approval Preset",
  description: "Choose the approval and sandboxing preset for this session",
  category: "mode",
  kind: {
    type: "select",
    current_value: "agent",
    groups: [],
    options: [
      {
        value: "read-only",
        name: "Read-only",
        description: "Requires approval to edit files and run commands.",
      },
      {
        value: "agent",
        name: "Agent",
        description: "Read and edit files, and run commands.",
      },
      {
        value: "agent-full-access",
        name: "Agent (full access)",
        description:
          "Can edit files outside this workspace and run commands with network access.",
      },
    ],
  },
}

const GROK_OPTIONS: SessionConfigOptionInfo[] = [
  {
    id: "model",
    name: "Model",
    description: "Choose the model for this session.",
    category: "model",
    kind: {
      type: "select",
      current_value: MANAGED_MODELS[0].value,
      options: MANAGED_MODELS,
      groups: [],
    },
  },
  {
    id: "reasoning_effort",
    name: "Reasoning effort",
    description: "Adjust how deeply the model reasons before responding.",
    category: "thought_level",
    kind: {
      type: "select",
      current_value: "medium",
      options: REASONING_OPTIONS,
      groups: [],
    },
  },
]

function snapshot(
  configOptions: SessionConfigOptionInfo[]
): AgentOptionsSnapshot {
  return { modes: null, config_options: configOptions, available_commands: [] }
}

const FIXED_AGENT_OPTIONS: Record<AgentType, AgentOptionsSnapshot> = {
  codex: snapshot([CODEX_MODE]),
  grok: snapshot(GROK_OPTIONS),
  claude_code: snapshot([]),
  hermes: snapshot([]),
  open_code: snapshot([]),
  open_claw: snapshot([]),
  code_buddy: snapshot([]),
  gemini: snapshot([]),
  cline: snapshot([]),
  kimi_code: snapshot([]),
  pi: snapshot([]),
}

export function getFixedAgentOptions(
  agentType: AgentType,
  configValues: Record<string, string> = {},
  translator?: SessionConfigTranslator
): AgentOptionsSnapshot {
  const base = FIXED_AGENT_OPTIONS[agentType]
  return {
    ...base,
    config_options: base.config_options.map((option) => {
      const selected = configValues[option.id]
      const configured =
        selected && option.kind.type === "select"
          ? {
              ...option,
              kind: { ...option.kind, current_value: selected },
            }
          : option
      return translator
        ? localizeSessionConfigOption(configured, translator)
        : configured
    }),
  }
}
