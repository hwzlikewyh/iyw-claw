import { describe, expect, it } from "vitest"

import type { AgentRuntimeErrorMessages } from "@/lib/agent-runtime-error"
import type { MessageTurn } from "@/lib/types"
import { adaptMessageTurn } from "./ai-elements-adapter"

const runtimeErrors: AgentRuntimeErrorMessages = {
  insufficientBalance: "余额不足，请充值后重试。",
  authenticationFailed: "认证失败，请检查账号配置。",
  permissionDenied: "当前账号没有访问权限。",
  rateLimited: "请求过于频繁，请稍后重试。",
  quotaExceeded: "可用额度已用完，请调整额度后重试。",
  modelUnavailable: "当前模型不可用，请开通或更换模型后重试。",
  requestTimeout: "请求超时，请稍后重试。",
  networkError: "网络连接失败，请检查网络后重试。",
  serviceUnavailable: "服务暂时不可用，请稍后重试。",
  requestFailed: "请求失败，请稍后重试。",
}

function assistantTurn(text: string): MessageTurn {
  return {
    id: "turn-1",
    role: "assistant",
    blocks: [{ type: "text", text }],
    timestamp: "2026-07-18T00:00:00.000Z",
  }
}

describe("adaptMessageTurn runtime errors", () => {
  it("localizes an unactivated-model response before it reaches the UI", () => {
    const raw =
      "unexpected status 404 Not Found: Your account 2102292408 has not " +
      "activated the model deepseek-v4-pro-260425. " +
      "url: https://gateway.iyw.cn/iyw-fusion-api/v1/responses, " +
      "request id: ab6fc2b2-6c01-4b44-af2e-6963b80fffbd"

    const adapted = adaptMessageTurn(assistantTurn(raw), {
      attachedResources: "附加资源",
      toolCallFailed: "工具调用失败",
      runtimeErrors,
    })

    expect(adapted.content).toEqual([
      {
        type: "text",
        text: "当前模型不可用，请开通或更换模型后重试。",
      },
    ])
  })
})
