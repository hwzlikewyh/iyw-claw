import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

const connection = vi.hoisted(() => ({
  backgroundOutstanding: 2,
  backgroundSettleSyncingSince: null as number | null,
}))

vi.mock("@/hooks/use-connection", () => ({
  useConnection: () => connection,
}))

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string, values?: Record<string, unknown>) =>
    values?.count == null ? key : `${key}:${values.count}`,
}))

import { BackgroundTasksChip } from "@/components/chat/background-tasks-chip"

describe("BackgroundTasksChip", () => {
  it("shows the outstanding background task count", () => {
    render(<BackgroundTasksChip contextKey="tab-1" />)

    expect(screen.getByText("running:2")).toBeTruthy()
  })
})
