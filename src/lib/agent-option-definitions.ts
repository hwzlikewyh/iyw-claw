import type {
  AgentType,
  SessionModeInfo,
  SessionModeStateInfo,
} from "@/lib/types"
import type { GatewayModel } from "@/lib/gateway-model-catalog"

type LocalModelDefinition = GatewayModel

export const AGENT_MODEL_IDS: Record<AgentType, readonly string[]> = {
  codex: ["gpt-5.4", "deepseek-v4-pro", "deepseek-v4-flash"],
  claude_code: ["claude-opus-4-6", "gpt-5.4"],
  gemini: ["gemini-3.1-pro-preview", "gpt-5.4"],
  grok: [
    "gpt-5.4",
    "claude-opus-4-6",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "doubao-seed-2-1-pro-260628",
    "gemini-3.1-pro-preview",
    "qwen3.7-max",
  ],
  hermes: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
  open_code: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
  open_claw: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
  code_buddy: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
  cline: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
  kimi_code: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
  pi: ["deepseek-v4-pro", "deepseek-v4-flash", "qwen3.7-max"],
}

const LOCAL_MODELS: readonly LocalModelDefinition[] = [
  {
    id: "gpt-5.4",
    name: "GPT-5.4",
    description: "通用对话、复杂推理、代码生成和工具调用",
    efforts: ["minimal", "low", "medium", "high", "xhigh"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    id: "claude-opus-4-6",
    name: "Claude Opus 4.6",
    description: "复杂推理、长上下文分析和高质量代码生成",
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    id: "deepseek-v4-pro",
    name: "DeepSeek V4 Pro",
    description: "深度推理、代码生成和多步骤工具调用",
    efforts: ["low", "medium", "high", "xhigh"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    id: "deepseek-v4-flash",
    name: "DeepSeek V4 Flash",
    description: "低延迟对话、快速推理和常规代码任务",
    efforts: ["low", "medium", "high"],
    defaultEffort: "medium",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    id: "doubao-seed-2-1-pro-260628",
    name: "豆包 Seed 2.1 Pro",
    description: "通用对话、内容生成和工具调用",
    efforts: ["minimal", "low", "medium", "high"],
    defaultEffort: "medium",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    id: "gemini-3.1-pro-preview",
    name: "Gemini 3.1 Pro Preview",
    description: "长文本理解、复杂分析和多模态扩展",
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    id: "qwen3.7-max",
    name: "通义千问 3.7 Max",
    description: "中文对话、知识问答、推理和代码生成",
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
]

const defaultMode = (description: string): SessionModeInfo => ({
  id: "default",
  name: "默认模式",
  description,
})

const AGENT_MODES: Record<AgentType, SessionModeInfo[]> = {
  codex: [
    {
      id: "read-only",
      name: "默认模式",
      description: "需要手动确认每个操作，适合谨慎使用",
    },
    { id: "plan", name: "规划模式", description: "只读分析并生成执行计划" },
    { id: "agent", name: "代理模式", description: "自动接受工作区内文件编辑" },
    {
      id: "agent-full-access",
      name: "自动模式",
      description: "允许完整文件和网络访问【谨慎使用】",
    },
  ],
  claude_code: [
    defaultMode("每个敏感操作前请求确认"),
    {
      id: "acceptEdits",
      name: "代理模式",
      description: "自动接受文件创建和编辑",
    },
    { id: "plan", name: "规划模式", description: "只读分析并生成执行计划" },
    {
      id: "bypassPermissions",
      name: "自动模式",
      description: "绕过权限检查【谨慎使用】",
    },
  ],
  gemini: [
    defaultMode("需要手动确认高风险操作"),
    { id: "auto_edit", name: "代理模式", description: "自动应用文件编辑" },
    { id: "yolo", name: "自动模式", description: "完全自动执行【谨慎使用】" },
  ],
  grok: [
    defaultMode("需要手动确认每个操作"),
    { id: "plan", name: "规划模式", description: "只读规划，不直接修改文件" },
    { id: "acceptEdits", name: "代理模式", description: "自动接受文件编辑" },
    { id: "auto", name: "自动编辑", description: "自动执行常规工具调用" },
    { id: "dontAsk", name: "免确认模式", description: "减少权限询问" },
    {
      id: "bypassPermissions",
      name: "自动模式",
      description: "绕过权限检查【谨慎使用】",
    },
  ],
  open_code: [
    { id: "plan", name: "规划模式", description: "只读分析和计划" },
    { id: "build", name: "代理模式", description: "执行代码修改和工具调用" },
  ],
  cline: [
    { id: "plan", name: "规划模式", description: "分析问题并准备计划" },
    { id: "act", name: "代理模式", description: "执行工具和文件修改" },
  ],
  code_buddy: [
    defaultMode("每个敏感操作前请求确认"),
    {
      id: "acceptEdits",
      name: "代理模式",
      description: "自动接受文件创建和编辑",
    },
    { id: "plan", name: "规划模式", description: "只读分析并生成执行计划" },
    {
      id: "bypassPermissions",
      name: "自动模式",
      description: "绕过权限检查【谨慎使用】",
    },
  ],
  hermes: [defaultMode("按 Agent 默认策略执行")],
  open_claw: [defaultMode("按 Agent 默认策略执行")],
  kimi_code: [defaultMode("按 Agent 默认策略执行")],
  pi: [defaultMode("按 Agent 默认策略执行")],
}

export function getLocalAgentModelIds(agentType: AgentType): string[] {
  return [...AGENT_MODEL_IDS[agentType]]
}

export function getLocalModels(agentType: AgentType): GatewayModel[] {
  const byId = new Map(LOCAL_MODELS.map((model) => [model.id, model]))
  return AGENT_MODEL_IDS[agentType].flatMap((id) => {
    const model = byId.get(id)
    return model ? [{ ...model, efforts: [...model.efforts] }] : []
  })
}

export function getAgentModeState(agentType: AgentType): SessionModeStateInfo {
  const availableModes = AGENT_MODES[agentType]
  return {
    current_mode_id: availableModes[0]?.id ?? "default",
    available_modes: availableModes,
  }
}
