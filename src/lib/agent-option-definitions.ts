import type {
  AgentType,
  BuiltinAgentType,
  SessionModeStateInfo,
} from "@/lib/types"
import { isCustomAgentType } from "@/lib/types"
import type { GatewayModel } from "@/lib/gateway-model-catalog"
import { getStaticAgentModeState } from "@/lib/agent-control-profiles"

type LocalModelDefinition = GatewayModel

const LOCAL_MODEL_CAPABILITY_DEFAULTS: Pick<
  GatewayModel,
  | "capabilities"
  | "imageInputMode"
  | "contextWindow"
  | "maxInputTokens"
  | "maxOutputTokens"
  | "compactionAtTokens"
> = {
  capabilities: {
    streaming: false,
    toolCalling: false,
    parallelToolCalling: false,
    webSearch: false,
    vision: false,
    audioInput: false,
    structuredOutput: false,
    promptCache: false,
    imageGeneration: false,
    imageEditing: false,
  },
  imageInputMode: "none",
  contextWindow: null,
  maxInputTokens: null,
  maxOutputTokens: null,
  compactionAtTokens: null,
}

export const AGENT_MODEL_IDS: Record<BuiltinAgentType, readonly string[]> = {
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
  cursor: [],
  deepseek: [],
}

const LOCAL_MODELS: readonly LocalModelDefinition[] = [
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "gpt-5.4",
    name: "GPT-5.4",
    description: "通用对话、复杂推理、代码生成和工具调用",
    efforts: ["minimal", "low", "medium", "high", "xhigh"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "claude-opus-4-6",
    name: "Claude Opus 4.6",
    description: "复杂推理、长上下文分析和高质量代码生成",
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "deepseek-v4-pro",
    name: "DeepSeek V4 Pro",
    description: "深度推理、代码生成和多步骤工具调用",
    efforts: ["low", "medium", "high", "xhigh"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "deepseek-v4-flash",
    name: "DeepSeek V4 Flash",
    description: "低延迟对话、快速推理和常规代码任务",
    efforts: ["low", "medium", "high"],
    defaultEffort: "medium",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "doubao-seed-2-1-pro-260628",
    name: "豆包 Seed 2.1 Pro",
    description: "通用对话、内容生成和工具调用",
    efforts: ["minimal", "low", "medium", "high"],
    defaultEffort: "medium",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "gemini-3.1-pro-preview",
    name: "Gemini 3.1 Pro Preview",
    description: "长文本理解、复杂分析和多模态扩展",
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
  {
    ...LOCAL_MODEL_CAPABILITY_DEFAULTS,
    id: "qwen3.7-max",
    name: "通义千问 3.7 Max",
    description: "中文对话、知识问答、推理和代码生成",
    efforts: ["low", "medium", "high"],
    defaultEffort: "high",
    fastModeSupported: false,
    fastModeDefaultEnabled: false,
  },
]

export function getLocalAgentModelIds(agentType: AgentType): string[] {
  if (isCustomAgentType(agentType)) return []
  return [...AGENT_MODEL_IDS[agentType]]
}

/** The gateway catalog is authoritative. Protocol compatibility belongs to
 * the fusion gateway; the payload has no per-agent capability field that
 * would let the client safely remove entries. */
export function deriveAgentModels(
  _agentType: AgentType,
  models: GatewayModel[]
): GatewayModel[] {
  return [...models]
}

export function getLocalModels(agentType: AgentType): GatewayModel[] {
  if (isCustomAgentType(agentType)) return []
  const byId = new Map(LOCAL_MODELS.map((model) => [model.id, model]))
  return AGENT_MODEL_IDS[agentType].flatMap((id) => {
    const model = byId.get(id)
    return model ? [{ ...model, efforts: [...model.efforts] }] : []
  })
}

export function getAgentModeState(agentType: AgentType): SessionModeStateInfo {
  return getStaticAgentModeState(agentType)
}
