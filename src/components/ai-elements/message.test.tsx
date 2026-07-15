import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import type { ReactNode } from "react"
import { describe, expect, it, vi } from "vitest"

vi.mock("streamdown", () => ({
  Streamdown: ({
    children,
    className,
  }: {
    children: ReactNode
    className?: string
  }) => (
    <div className={className} data-testid="streamdown-root">
      {children}
    </div>
  ),
  defaultRemarkPlugins: {},
  defaultRehypePlugins: {},
}))

vi.mock("@streamdown/cjk", () => ({ cjk: {} }))
vi.mock("@streamdown/math", () => ({
  createMathPlugin: () => ({}),
}))
vi.mock("@streamdown/mermaid", () => ({ mermaid: {} }))
vi.mock("@streamdown/code", () => ({
  code: {
    highlight: vi.fn(),
    supportsLanguage: vi.fn(() => true),
  },
}))

vi.mock("@/components/ai-elements/link-safety", () => ({
  useStreamdownLinkSafety: () => ({ enabled: false }),
}))

import { MessageResponse } from "./message"
import { Reasoning, ReasoningContent, ReasoningTrigger } from "./reasoning"
import enMessages from "@/i18n/messages/en.json"

describe("MessageResponse", () => {
  it("applies marker styles so ordered Markdown lists render as lists", () => {
    render(<MessageResponse>{"1. First\n2. Second"}</MessageResponse>)

    expect(screen.getByTestId("streamdown-root")).toHaveClass(
      "[&_ol]:list-decimal",
      "[&_ol]:pl-3"
    )
  })
})

describe("Reasoning", () => {
  it("stays collapsed when the user closes it during streaming", async () => {
    render(
      <NextIntlClientProvider locale="en" messages={enMessages}>
        <Reasoning isStreaming>
          <ReasoningTrigger />
          <ReasoningContent>private reasoning</ReasoningContent>
        </Reasoning>
      </NextIntlClientProvider>
    )

    const trigger = screen.getByRole("button")
    expect(trigger).toHaveAttribute("aria-expanded", "true")

    fireEvent.click(trigger)

    await waitFor(() => {
      expect(trigger).toHaveAttribute("aria-expanded", "false")
    })
  })
})
