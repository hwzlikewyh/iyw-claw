import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

vi.mock("next-intl", () => ({
  useTranslations: (namespace: string) => (key: string) =>
    `${namespace}.${key}`,
}))

vi.mock("@/components/message/agent-capsule", () => ({
  AgentCapsule: ({
    statusLabel,
    children,
  }: {
    statusLabel?: string
    children: React.ReactNode
  }) => (
    <div>
      <span>{statusLabel}</span>
      {children}
    </div>
  ),
}))

vi.mock("@/components/ai-elements/message", () => ({
  MessageResponse: ({ children }: { children: React.ReactNode }) => (
    <div>{children}</div>
  ),
}))

vi.mock("@/components/ai-elements/shimmer", () => ({
  Shimmer: ({ children }: { children: React.ReactNode }) => (
    <span>{children}</span>
  ),
}))

import { AgentToolCallPart } from "@/components/message/agent-tool-call"
import { BACKGROUND_TASK_MARKER } from "@/lib/background-agent"

function renderPart(output: string) {
  render(
    <AgentToolCallPart
      part={
        {
          type: "tool-call",
          toolCallId: "toolu_01",
          toolName: "Task",
          input: '{"description":"Run build"}',
          state: "output-available",
          output,
        } as never
      }
      renderToolCall={() => null}
    />
  )
}

describe("AgentToolCallPart background lifecycle", () => {
  it("renders a live async launch as background work without the raw ack", () => {
    const ack = "Async agent launched successfully. agentId: agent-1"
    renderPart(ack)

    expect(
      screen.getAllByText("Folder.chat.backgroundTasks.cardRunning")
    ).not.toHaveLength(0)
    expect(screen.queryByText(ack)).toBeNull()
  })

  it("renders a settled marker as a lifecycle result without marker JSON", () => {
    const marker = `${BACKGROUND_TASK_MARKER}{"task_id":"agent-1","status":"completed","summary":"Build finished","result":"Build OK"}`
    renderPart(marker)

    expect(
      screen.getByText("Folder.chat.backgroundTasks.cardCompleted")
    ).toBeTruthy()
    expect(screen.getByText("Build finished")).toBeTruthy()
    expect(screen.getByText("Build OK")).toBeTruthy()
    expect(screen.queryByText(marker)).toBeNull()
  })
})
