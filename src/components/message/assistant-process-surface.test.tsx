import { fireEvent, render, screen } from "@testing-library/react"
import { act } from "react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"

import messages from "@/i18n/messages/zh-CN.json"

import { AssistantProcessSurface } from "./assistant-process-surface"

const reasoningPart = {
  type: "reasoning" as const,
  content: "正在分析现有实现。",
  isStreaming: true,
}

function ProcessSurface({
  complete = false,
  hasError = false,
}: {
  complete?: boolean
  hasError?: boolean
}) {
  return (
    <NextIntlClientProvider locale="zh-CN" messages={messages}>
      <AssistantProcessSurface
        parts={[{ ...reasoningPart, isStreaming: !complete }]}
        processCount={1}
        processHasError={hasError}
        entranceKey="test-turn"
        animationEnabled={false}
        isResponseComplete={complete}
        displayMode="summary"
        collapseCompletedTurn
        autoOpenErrors={false}
        conversationId={1}
        durationMs={2_000}
      />
    </NextIntlClientProvider>
  )
}

describe("AssistantProcessSurface", () => {
  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0)
      return 1
    })
    vi.stubGlobal("cancelAnimationFrame", vi.fn())
  })

  it("renders one live reasoning surface without nested reasoning triggers", () => {
    render(<ProcessSurface />)

    expect(screen.getAllByText("思考中…")).toHaveLength(1)
    expect(screen.queryByText("思考")).toBeNull()
    expect(screen.getByText("正在分析现有实现。")).not.toBeNull()
  })

  it("starts completed history collapsed and opens on demand", () => {
    render(<ProcessSurface complete />)
    const trigger = screen.getByRole("button")

    expect(trigger.getAttribute("data-state")).toBe("closed")
    expect(screen.queryByText("正在分析现有实现。")).toBeNull()

    fireEvent.click(trigger)
    expect(trigger.getAttribute("data-state")).toBe("open")
    expect(screen.getByText("正在分析现有实现。")).not.toBeNull()
  })

  it("settles a live process into the collapsed completed state", () => {
    vi.useFakeTimers()
    const view = render(<ProcessSurface />)
    const trigger = screen.getByRole("button")
    expect(trigger.getAttribute("data-state")).toBe("open")

    view.rerender(<ProcessSurface complete />)
    expect(trigger.getAttribute("data-state")).toBe("open")
    act(() => vi.advanceTimersByTime(500))

    expect(trigger.getAttribute("data-state")).toBe("closed")
    expect(screen.queryByText("正在分析现有实现。")).toBeNull()
    vi.useRealTimers()
  })

  it("keeps error details inside the process stream without a summary badge", () => {
    render(<ProcessSurface complete hasError />)

    expect(screen.queryByText("包含错误")).toBeNull()
  })
})
