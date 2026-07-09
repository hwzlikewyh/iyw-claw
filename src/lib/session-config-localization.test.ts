import { describe, expect, it } from "vitest"
import type { SessionConfigOptionInfo } from "@/lib/types"
import { localizeSessionConfigOption } from "@/lib/session-config-localization"

const messages: Record<string, string> = {
  "sessionConfig.options.mode.name": "权限模式",
  "sessionConfig.options.mode.description": "选择权限预设",
  "sessionConfig.values.mode.readOnly.name": "只读",
  "sessionConfig.values.mode.readOnly.description": "需要审批",
  "sessionConfig.values.mode.agent.name": "智能体",
  "sessionConfig.values.mode.agent.description": "可读写和运行命令",
  "sessionConfig.values.mode.agentFullAccess.name": "智能体（完全访问）",
  "sessionConfig.values.mode.agentFullAccess.description": "可完全访问",
  "sessionConfig.options.reasoning.name": "推理强度",
  "sessionConfig.values.reasoning.xhigh.name": "极高",
  "sessionConfig.values.reasoning.xhigh.description": "最高推理深度",
  "sessionConfig.values.switch.on": "开启",
  "sessionConfig.values.switch.onDescription": "速度更快，用量增加",
  "sessionConfig.values.switch.off": "关闭",
  "sessionConfig.values.switch.offDescription": "默认速度，正常用量",
}

function t(key: string): string {
  return messages[key] ?? key
}

function selectOption(
  overrides: Partial<SessionConfigOptionInfo>
): SessionConfigOptionInfo {
  return {
    id: "mode",
    name: "Approval Preset",
    description: "Choose an approval and sandboxing preset",
    category: "mode",
    kind: {
      type: "select",
      current_value: "agent",
      options: [],
      groups: [],
    },
    ...overrides,
  }
}

describe("localizeSessionConfigOption", () => {
  it("localizes Codex approval presets without changing values", () => {
    const option = selectOption({
      kind: {
        type: "select",
        current_value: "agent-full-access",
        options: [
          {
            value: "read-only",
            name: "Read-only",
            description: "Requires approval",
          },
          { value: "agent", name: "Agent", description: "Read and edit" },
          {
            value: "agent-full-access",
            name: "Agent (full access)",
            description: "Full access",
          },
        ],
        groups: [],
      },
    })

    const localized = localizeSessionConfigOption(option, t)

    expect(localized.name).toBe("权限模式")
    expect(localized.kind.options.map((item) => item.value)).toEqual([
      "read-only",
      "agent",
      "agent-full-access",
    ])
    expect(localized.kind.options.map((item) => item.name)).toEqual([
      "只读",
      "智能体",
      "智能体（完全访问）",
    ])
  })

  it("localizes reasoning levels and descriptions", () => {
    const option = selectOption({
      id: "reasoning_effort",
      name: "Reasoning effort",
      description: null,
      category: "thought_level",
      kind: {
        type: "select",
        current_value: "xhigh",
        options: [
          {
            value: "xhigh",
            name: "Extra high",
            description: "Extra high reasoning depth",
          },
        ],
        groups: [],
      },
    })

    const localized = localizeSessionConfigOption(option, t)

    expect(localized.name).toBe("推理强度")
    expect(localized.kind.options[0]).toMatchObject({
      value: "xhigh",
      name: "极高",
      description: "最高推理深度",
    })
  })

  it("localizes generic on/off selectors but leaves model names intact", () => {
    const switchOption = selectOption({
      id: "web_search",
      name: "Web Search",
      category: null,
      kind: {
        type: "select",
        current_value: "off",
        options: [
          { value: "on", name: "On", description: "Fast" },
          { value: "off", name: "Off", description: "Default" },
        ],
        groups: [],
      },
    })
    const modelOption = selectOption({
      id: "model",
      name: "Model",
      category: "model",
      kind: {
        type: "select",
        current_value: "openai/gpt-5",
        options: [{ value: "openai/gpt-5", name: "GPT-5", description: null }],
        groups: [],
      },
    })

    expect(localizeSessionConfigOption(switchOption, t).kind.options).toEqual([
      { value: "on", name: "开启", description: "速度更快，用量增加" },
      { value: "off", name: "关闭", description: "默认速度，正常用量" },
    ])
    expect(
      localizeSessionConfigOption(modelOption, t).kind.options[0].name
    ).toBe("GPT-5")
  })
})
