import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { useTabStore, type TabItem } from "@/contexts/tab-context"
import { usePluginWorkspace } from "./use-plugin-workspace"
import { usePluginComposerContext } from "@/lib/plugin-composer-context"

const mocks = vi.hoisted(() => ({
  prepare: vi.fn(),
  transport: {},
  onNew: null as null | (() => void),
}))
vi.mock("next-intl", () => ({ useTranslations: () => (key: string) => key }))
vi.mock("@/lib/transport", () => ({ getTransport: () => mocks.transport }))
vi.mock("@/lib/conversation-connectors", () => ({
  prepareConnectorConversation: mocks.prepare,
}))
vi.mock("@/stores/tab-store", () => ({
  onNewConversationOpened: (listener: () => void) => {
    mocks.onNew = listener
    return () => {
      mocks.onNew = null
    }
  },
}))
vi.mock("@/contexts/tab-context", async () => {
  const { create } = await import("zustand")
  return {
    useTabStore: create(() => ({
      activeTabId: "draft",
      rawTabs: [] as Array<{
        id: string
        conversationId: number | null
        agentType: string
        folderId: number
        isChat: boolean
        workingDir: string
      }>,
      draftRetargetRequests: [] as Array<{ tabId: string }>,
    })),
  }
})
const draft: TabItem = {
  id: "draft",
  kind: "conversation",
  title: "Plugin verification",
  isPinned: false,
  conversationId: null,
  agentType: "codex",
  folderId: 1,
  isChat: true,
  workingDir: "D:/workspace/original",
}
beforeEach(() => {
  vi.clearAllMocks()
  usePluginComposerContext.setState({ entries: {} })
  useTabStore.setState({
    activeTabId: "draft",
    rawTabs: [{ ...draft }],
    draftRetargetRequests: [],
  })
})
function pending() {
  let finish!: (value: { id: number; bind: () => void }) => void
  let fail!: (cause: unknown) => void
  mocks.prepare.mockReturnValue(
    new Promise((resolve, reject) => {
      finish = resolve
      fail = reject
    })
  )
  const bind = vi.fn(() =>
    useTabStore.setState({ rawTabs: [{ ...draft, conversationId: 42 }] })
  )
  return { finish: () => finish({ id: 42, bind }), fail, bind }
}
describe("plugin workspace binding", () => {
  it("reports a current preparation failure and clears it on retry", async () => {
    mocks.prepare.mockRejectedValueOnce(Error("preparation failed"))
    const view = renderHook(() => usePluginWorkspace("draft"))
    await act(async () => {
      expect(await view.result.current.bind()).toBeNull()
    })
    expect(view.result.current.error).toContain("preparation failed")
    const task = pending()
    let first!: Promise<number | null>
    act(() => {
      first = view.result.current.bind()
    })
    await act(async () => {
      task.finish()
      expect(await first).toBe(42)
    })
    expect(view.result.current.error).toBe("")
    view.unmount()
  })
  it("ignores a preparation error after the original directory changes", async () => {
    const task = pending(),
      view = renderHook(() => usePluginWorkspace("draft"))
    let first!: Promise<number | null>
    act(() => {
      first = view.result.current.bind()
    })
    act(() =>
      useTabStore.setState({
        rawTabs: [{ ...draft, workingDir: "D:/workspace/other" }],
      })
    )
    await act(async () => {
      task.fail(Error("old preparation failed"))
      expect(await first).toBeNull()
    })
    expect(view.result.current.error).toBe("")
    expect(task.bind).not.toHaveBeenCalled()
    view.unmount()
  })
  it("binds once and returns the actual stored conversation", async () => {
    const task = pending(),
      view = renderHook(() => usePluginWorkspace("draft"))
    let first!: Promise<number | null>
    act(() => {
      first = view.result.current.bind()
    })
    await act(async () => {
      expect(await view.result.current.bind()).toBeNull()
    })
    await act(async () => {
      task.finish()
      expect(await first).toBe(42)
    })
    expect(mocks.prepare).toHaveBeenCalledTimes(1)
    expect(task.bind).toHaveBeenCalledTimes(1)
    expect(
      usePluginComposerContext.getState().entries.draft?.preparedConversationId
    ).toBe(42)
    view.unmount()
  })
  it("reuses a conversation already bound by a concurrent message", async () => {
    const task = pending(),
      view = renderHook(() => usePluginWorkspace("draft"))
    let first!: Promise<number | null>
    act(() => {
      first = view.result.current.bind()
    })
    act(() =>
      useTabStore.setState({ rawTabs: [{ ...draft, conversationId: 99 }] })
    )
    await act(async () => {
      task.finish()
      expect(await first).toBe(99)
    })
    expect(task.bind).not.toHaveBeenCalled()
    expect(usePluginComposerContext.getState().entries.draft).toBeUndefined()
    view.unmount()
  })
  it("cancels after a working directory changes away and back", async () => {
    const task = pending(),
      view = renderHook(() => usePluginWorkspace("draft"))
    let first!: Promise<number | null>
    act(() => {
      first = view.result.current.bind()
    })
    act(() => {
      useTabStore.setState({
        rawTabs: [{ ...draft, workingDir: "D:/workspace/other" }],
      })
      useTabStore.setState({ rawTabs: [{ ...draft }] })
    })
    await act(async () => {
      task.finish()
      expect(await first).toBeNull()
    })
    expect(task.bind).not.toHaveBeenCalled()
    view.unmount()
  })
})
