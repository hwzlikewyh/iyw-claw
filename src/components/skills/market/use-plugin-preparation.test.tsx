import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { usePluginPreparation } from "./use-plugin-preparation"
import {
  readPluginPreparation,
  changePluginPreparation,
} from "@/lib/plugin-preparation"

vi.mock("@/lib/transport", () => ({ getTransport: () => ({}) }))
vi.mock("@/lib/plugin-preparation", () => ({
  readPluginPreparation: vi.fn(),
  changePluginPreparation: vi.fn(),
  checkPluginPreparation: vi.fn(),
}))
vi.mock("@/lib/app-error", () => ({ extractAppCommandError: () => null }))
const view = {
  slug: "iyw-pdfcraft",
  installedVersion: "0.1.0",
  schemaVersion: 2,
  state: "permission_required",
  contextToken: "scope",
  issues: [],
  connectors: [],
  permissionGranted: false,
} as never
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(readPluginPreparation).mockResolvedValue(view)
})
describe("plugin preparation recovery", () => {
  it("clears a prior failure after a successful refreshed state", async () => {
    vi.mocked(readPluginPreparation).mockRejectedValueOnce(
      Error("temporarily unavailable")
    )
    const { result, unmount } = renderHook(() =>
      usePluginPreparation("plugin", 42)
    )
    await waitFor(() =>
      expect(result.current.error).toContain("temporarily unavailable")
    )
    act(() => result.current.refresh())
    await waitFor(() => expect(result.current.view).toBe(view))
    expect(result.current.error).toBe("")
    unmount()
  })
  it("ignores a read failure arriving after a newer permission update", async () => {
    let fail!: (cause: unknown) => void
    vi.mocked(readPluginPreparation).mockReturnValue(
      new Promise((_resolve, reject) => {
        fail = reject
      })
    )
    const updated = {
      ...(view as object),
      permissionGranted: true,
      state: "unchecked",
    } as never
    vi.mocked(changePluginPreparation).mockResolvedValue(updated)
    const { result, unmount } = renderHook(() =>
      usePluginPreparation("plugin", 42)
    )
    await act(async () => {
      await result.current.mutate(view, true)
    })
    await act(async () => {
      fail(Error("stale read"))
    })
    expect(result.current.view).toBe(updated)
    expect(result.current.error).toBe("")
    unmount()
  })
})
