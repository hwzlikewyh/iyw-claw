import type {
  AgentType,
  BuiltinAgentType,
  SessionConfigOptionInfo,
  SessionModeInfo,
  SessionModeStateInfo,
} from "@/lib/types"
import { isCustomAgentType } from "@/lib/types"

interface StaticSelectControl {
  id: string
  name: string
  description: string
  category: string
  defaultValue: string
  options: SessionModeInfo[]
}

interface AgentControlProfile {
  modes: SessionModeInfo[]
  configOptions?: StaticSelectControl[]
  effortConfigId?: string
  fastConfigId?: string
}

const item = (
  id: string,
  name: string,
  description: string
): SessionModeInfo => ({ id, name, description })

const manual = item("default", "手动确认", "危险操作前请求确认")
const acceptEdits = item("acceptEdits", "接受编辑", "自动接受文件创建和编辑")
const plan = item("plan", "规划模式", "只读分析并生成执行计划")
const bypass = item(
  "bypassPermissions",
  "完全放行",
  "绕过所有权限检查【谨慎使用】"
)

const PROFILES: Record<BuiltinAgentType, AgentControlProfile> = {
  codex: {
    modes: [
      item("read-only", "只读", "编辑文件和运行命令前需要确认"),
      item("agent", "智能体", "可读取和编辑工作区文件，并运行命令"),
      item(
        "agent-full-access",
        "完全访问",
        "可编辑工作区外文件并使用网络【谨慎使用】"
      ),
    ],
    configOptions: [
      {
        id: "collaboration_mode",
        name: "工作方式",
        description: "选择 Codex 在后续轮次中的协作方式",
        category: "collaboration_mode",
        defaultValue: "default",
        options: [
          item("default", "标准协作", "根据任务直接分析或执行"),
          item("plan", "先规划", "先形成计划，再进行修改"),
        ],
      },
    ],
    effortConfigId: "reasoning_effort",
    fastConfigId: "fast-mode",
  },
  claude_code: {
    modes: [
      item("auto", "智能审批", "由模型判断是否批准权限请求"),
      manual,
      acceptEdits,
      item("plan", "规划模式", "只规划，不实际执行工具"),
      item("dontAsk", "不询问", "不弹出权限询问，未预批准则拒绝"),
      bypass,
    ],
    effortConfigId: "effort",
    fastConfigId: "fast",
  },
  gemini: {
    modes: [
      item("default", "手动确认", "高风险操作前请求确认"),
      item("auto_edit", "自动编辑", "自动应用文件编辑"),
      item("yolo", "完全自动", "自动执行所有操作【谨慎使用】"),
    ],
  },
  grok: {
    modes: [
      manual,
      item("plan", "规划模式", "只读规划，不直接修改文件"),
      acceptEdits,
      item("auto", "自动执行", "自动执行常规工具调用"),
      item("dontAsk", "不询问", "不弹出权限询问"),
      bypass,
    ],
    effortConfigId: "reasoning_effort",
  },
  open_code: {
    modes: [
      item("plan", "规划", "只读分析和计划"),
      item("build", "执行", "执行代码修改和工具调用"),
    ],
  },
  cline: {
    modes: [
      item("plan", "规划", "分析问题并准备计划"),
      item("act", "行动", "执行工具和文件修改"),
    ],
  },
  code_buddy: { modes: [manual, acceptEdits, plan, bypass] },
  pi: { modes: [], effortConfigId: "thought_level" },
  hermes: { modes: [] },
  open_claw: { modes: [] },
  kimi_code: { modes: [] },
  cursor: { modes: [] },
  deepseek: { modes: [] },
}

function profile(agentType: AgentType): AgentControlProfile | null {
  return isCustomAgentType(agentType) ? null : PROFILES[agentType]
}

function currentValue(
  control: StaticSelectControl,
  configValues: Record<string, string>
): string {
  const configured = configValues[control.id]
  return configured && control.options.some(({ id }) => id === configured)
    ? configured
    : control.defaultValue
}

export function getStaticAgentModeState(
  agentType: AgentType
): SessionModeStateInfo {
  const modes = profile(agentType)?.modes ?? []
  return {
    current_mode_id: modes[0]?.id ?? "default",
    available_modes: modes.map((mode) => ({ ...mode })),
  }
}

export function getStaticAgentConfigOptions(
  agentType: AgentType,
  configValues: Record<string, string>
): SessionConfigOptionInfo[] {
  return (profile(agentType)?.configOptions ?? []).map((control) => ({
    id: control.id,
    name: control.name,
    description: control.description,
    category: control.category,
    kind: {
      type: "select",
      current_value: currentValue(control, configValues),
      options: control.options.map((option) => ({
        value: option.id,
        name: option.name,
        description: option.description,
      })),
      groups: [],
    },
  }))
}

export function getAgentModelBehaviorIds(agentType: AgentType): {
  effortConfigId: string | null
  fastConfigId: string | null
} {
  const current = profile(agentType)
  return {
    effortConfigId: current?.effortConfigId ?? null,
    fastConfigId: current?.fastConfigId ?? null,
  }
}
