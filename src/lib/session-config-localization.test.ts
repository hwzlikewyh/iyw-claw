import { describe, expect, it } from "vitest"

import { localizeSessionConfigOption } from "@/lib/session-config-localization"
import type { SessionConfigOptionInfo } from "@/lib/types"

const translations: Record<string, string> = {
  "sessionConfig.options.responseMode.name": "响应模式",
  "sessionConfig.options.responseMode.description": "选择响应速度。",
  "sessionConfig.values.responseMode.standard.name": "标准",
  "sessionConfig.values.responseMode.standard.description": "模型标准响应速度",
  "sessionConfig.values.responseMode.fast.name": "Fast",
  "sessionConfig.values.responseMode.fast.description":
    "模型快速响应模式（会产生额外消耗）",
}

const translate = (key: string) => translations[key] ?? key

describe("session config localization", () => {
  it("localizes Codex fast mode as a response speed selector", () => {
    const option: SessionConfigOptionInfo = {
      id: "fast-mode",
      name: "Fast mode",
      description: "1.5x speed, increased usage",
      category: "model_config",
      kind: {
        type: "select",
        current_value: "off",
        options: [
          {
            value: "off",
            name: "Off",
            description: "Default speed, normal usage",
          },
          {
            value: "on",
            name: "On",
            description: "1.5x speed, increased usage",
          },
        ],
        groups: [],
      },
    }

    expect(localizeSessionConfigOption(option, translate)).toMatchObject({
      name: "响应模式",
      description: "选择响应速度。",
      kind: {
        current_value: "off",
        options: [
          {
            value: "off",
            name: "标准",
            description: "模型标准响应速度",
          },
          {
            value: "on",
            name: "Fast",
            description: "模型快速响应模式（会产生额外消耗）",
          },
        ],
      },
    })
  })

  it("localizes the Claude ACP Fast option alias", () => {
    const option: SessionConfigOptionInfo = {
      id: "fast",
      name: "Fast",
      description: "Faster responses on supported models",
      category: null,
      kind: {
        type: "select",
        current_value: "off",
        options: [
          { value: "off", name: "Off" },
          { value: "on", name: "On" },
        ],
        groups: [],
      },
    }

    expect(localizeSessionConfigOption(option, translate)).toMatchObject({
      name: "响应模式",
      kind: {
        options: [{ name: "标准" }, { name: "Fast" }],
      },
    })
  })
})
