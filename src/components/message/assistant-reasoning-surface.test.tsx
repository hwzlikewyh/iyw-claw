import { fireEvent, render, screen } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { describe, expect, it, vi } from "vitest"

import messages from "@/i18n/messages/zh-CN.json"

import { AssistantReasoningSurface } from "./assistant-reasoning-surface"

const parts = [
  {
    type: "reasoning" as const,
    content: "先检查输入，再确认输出边界。",
    isStreaming: true,
  },
]

function Preview({ complete }: { complete: boolean }) {
  return (
    <NextIntlClientProvider locale="zh-CN" messages={messages}>
      <AssistantReasoningSurface parts={parts} isResponseComplete={complete} />
    </NextIntlClientProvider>
  )
}

describe("AssistantReasoningSurface", () => {
  it("keeps completed deep thinking closed until requested", () => {
    render(<Preview complete />)
    const trigger = screen.getByRole("button", { name: "深度思考" })

    expect(trigger.getAttribute("data-state")).toBe("closed")
    expect(screen.queryByText("先检查输入，再确认输出边界。")).toBeNull()

    fireEvent.click(trigger)
    expect(screen.getByText("先检查输入，再确认输出边界。")).not.toBeNull()
  })

  it("opens live deep thinking and exposes a scrollable viewport", () => {
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0)
      return 1
    })
    vi.stubGlobal("cancelAnimationFrame", vi.fn())
    render(<Preview complete={false} />)

    const trigger = screen.getByRole("button", { name: "深度思考" })
    expect(trigger.getAttribute("data-state")).toBe("open")
    expect(screen.getByText("先检查输入，再确认输出边界。")).not.toBeNull()
    expect(
      document.querySelector(".assistant-reasoning-content")
    ).not.toBeNull()
  })
})
