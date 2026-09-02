import { fireEvent, render, screen } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"

import messages from "@/i18n/messages/zh-CN.json"

import { AssistantTurnContent } from "./assistant-turn-content"

describe("AssistantTurnContent", () => {
  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0)
      return 1
    })
    vi.stubGlobal("cancelAnimationFrame", vi.fn())
  })

  it("keeps process and deep thinking independently collapsed", () => {
    render(
      <NextIntlClientProvider locale="zh-CN" messages={messages}>
        <AssistantTurnContent
          agentType="codex"
          parts={[
            {
              type: "reasoning",
              content: "这是独立的思考内容。",
              isStreaming: false,
            },
            { type: "text", text: "这是执行过程说明。" },
            { type: "text", text: "这是始终显示的最终答复。" },
          ]}
          entranceKey="completed-turn"
          animationEnabled={false}
          isResponseComplete
          displayMode="summary"
          collapseCompletedTurn
          autoOpenErrors={false}
          conversationId={1}
          durationMs={3_000}
        />
      </NextIntlClientProvider>
    )

    expect(screen.getByText("这是始终显示的最终答复。")).not.toBeNull()
    expect(screen.queryByText("这是执行过程说明。")).toBeNull()
    expect(screen.queryByText("这是独立的思考内容。")).toBeNull()

    const process = screen.getByRole("button", { name: /已完成 3 秒/ })
    const reasoning = screen.getByRole("button", { name: "深度思考" })
    fireEvent.click(process)
    expect(screen.getByText("这是执行过程说明。")).not.toBeNull()
    expect(screen.queryByText("这是独立的思考内容。")).toBeNull()

    fireEvent.click(reasoning)
    expect(screen.getByText("这是独立的思考内容。")).not.toBeNull()
  })

  it("keeps live body text inside the ordered process viewport", () => {
    render(
      <NextIntlClientProvider locale="zh-CN" messages={messages}>
        <AssistantTurnContent
          agentType="codex"
          parts={[
            {
              type: "tool-call",
              toolCallId: "read",
              toolName: "Read",
              input: null,
              output: null,
              state: "input-available",
            },
            { type: "text", text: "正在整理最终答复。" },
          ]}
          entranceKey="live-turn"
          animationEnabled={false}
          isResponseComplete={false}
          displayMode="summary"
          collapseCompletedTurn
          autoOpenErrors={false}
          conversationId={1}
        />
      </NextIntlClientProvider>
    )

    const response = screen.getByText("正在整理最终答复。")
    expect(response.closest(".assistant-process-viewport")).not.toBeNull()
    expect(document.querySelector(".assistant-process-viewport")).not.toBeNull()
  })
})
