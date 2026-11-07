import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { useNewConversationBrowser } from "./use-new-conversation-browser"

const mocks = vi.hoisted(() => ({
  closeBrowser: vi.fn(async () => {}),
  openBrowser: vi.fn(),
  setOpen: vi.fn(),
  receive: null as null | ((tabId: string) => void),
}))
vi.mock("@/contexts/browser-context", () => ({ useBrowser: () => mocks }))
vi.mock("@/contexts/aux-panel-context", () => ({
  useAuxPanelContext: () => mocks,
}))
vi.mock("@/stores/tab-store", () => ({
  onNewConversationOpened: (callback: (tabId: string) => void) => {
    mocks.receive = callback
    return () => {
      mocks.receive = null
    }
  },
}))

describe("new-conversation panel visibility", () => {
  it("only collapses on an explicit successful new-conversation event", () => {
    vi.clearAllMocks()
    const { rerender, unmount } = renderHook(useNewConversationBrowser)
    expect(mocks.setOpen).not.toHaveBeenCalled()
    act(() => mocks.receive?.("draft-1"))
    expect(mocks.setOpen).toHaveBeenCalledTimes(1)
    expect(mocks.setOpen).toHaveBeenCalledWith(false)
    expect(mocks.closeBrowser).toHaveBeenCalledTimes(1)
    expect(mocks.openBrowser).not.toHaveBeenCalled()
    rerender()
    expect(mocks.closeBrowser).toHaveBeenCalledTimes(1)
    unmount()
    expect(mocks.receive).toBeNull()
  })
})
