import { renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useTaskArtifacts } from "./use-task-artifacts"

const { listTaskArtifacts, subscribe, onTransportReconnect } = vi.hoisted(
  () => ({
    listTaskArtifacts: vi.fn(),
    subscribe: vi.fn(),
    onTransportReconnect: vi.fn(),
  })
)

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))

vi.mock("@/lib/api", () => ({ listTaskArtifacts }))

vi.mock("@/lib/platform", () => ({
  subscribe,
  onTransportReconnect,
}))

describe("useTaskArtifacts", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listTaskArtifacts.mockResolvedValue([])
    subscribe.mockResolvedValue(vi.fn())
    onTransportReconnect.mockReturnValue(vi.fn())
  })

  it("loads current artifacts after a draft receives its persisted ID", async () => {
    const { rerender } = renderHook(
      ({ conversationId }: { conversationId: number | null }) =>
        useTaskArtifacts({
          conversationId,
          folderId: null,
          scope: "current",
          latestTurnOnly: true,
        }),
      { initialProps: { conversationId: null as number | null } }
    )

    await waitFor(() => expect(listTaskArtifacts).not.toHaveBeenCalled())

    rerender({ conversationId: 152 })

    await waitFor(() =>
      expect(listTaskArtifacts).toHaveBeenCalledWith({
        conversationId: 152,
        latestTurnOnly: true,
      })
    )
  })
})
