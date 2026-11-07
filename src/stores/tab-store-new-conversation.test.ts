import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { canOpenConversationTab } from "@/lib/conversation-tab-preferences"
import {
  onNewConversationOpened,
  resetTabStore,
  useTabStore,
} from "./tab-store"

vi.mock("@/stores/app-workspace-store", async () => {
  const { create } = await import("zustand")
  return {
    useAppWorkspaceStore: create(() => ({
      conversations: [],
      allFolders: [],
      folders: [],
      setActiveFolderId: vi.fn(),
    })),
  }
})
vi.mock("@/lib/api", () => ({ getFolderConversation: vi.fn() }))
vi.mock("@/lib/conversation-title", () => ({
  formatConversationTitle: (value: string) => value,
}))
vi.mock("@/lib/conversation-window-state", () => ({
  isConversationWindowPending: () => false,
}))
vi.mock("@/lib/conversation-tab-preferences", () => ({
  canOpenConversationTab: vi.fn(() => true),
}))
vi.mock("@/lib/resolve-default-agent", () => ({
  resolveDefaultAgent: () => ({ agentType: "codex", provisional: false }),
}))

describe("successful new-conversation notifications", () => {
  const opened = vi.fn()
  const disconnect = vi.fn<() => Promise<boolean>>()
  let unsubscribe: () => void
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(canOpenConversationTab).mockReturnValue(true)
    useAppWorkspaceStore.setState({ allFolders: [], conversations: [] })
    resetTabStore()
    disconnect.mockResolvedValue(true)
    useTabStore.getState().setSideEffects({
      activateConversationPane: vi.fn(),
      acpDisconnect: async () => {},
      acpDisconnectForReplacement: disconnect,
    })
    unsubscribe = onNewConversationOpened(opened)
  })
  afterEach(() => unsubscribe())

  it("notifies for a new project draft and reuse of its ID", () => {
    const store = useTabStore.getState()
    store.openNewConversationTab(1, "C:/project")
    const id = useTabStore.getState().activeTabId
    store.openNewConversationTab(1, "C:/project")
    expect(opened.mock.calls).toEqual([[id], [id]])
    expect(useTabStore.getState().rawTabs).toHaveLength(1)
  })

  it("notifies for a folderless draft and its explicit reuse", () => {
    useTabStore.getState().openChatModeTab()
    const id = useTabStore.getState().activeTabId
    useTabStore.getState().openChatModeTab()
    expect(opened.mock.calls).toEqual([[id], [id]])
  })

  it("does not notify on initialization, rerender data, or history selection", () => {
    const store = useTabStore.getState()
    store.hydrate()
    store.recoverActiveContext()
    store.setLabels({
      loadingConversation: "loading",
      newConversation: "new",
      untitledConversation: "untitled",
    })
    store.openTab(1, 42, "codex", true, "history")
    store.switchTab(useTabStore.getState().rawTabs[0].id)
    expect(opened).not.toHaveBeenCalled()
  })

  it("does not notify when the tab limit rejects creation", () => {
    vi.mocked(canOpenConversationTab).mockReturnValue(false)
    useTabStore.getState().openNewConversationTab(1, "C:/project")
    useTabStore.getState().openChatModeTab()
    expect(opened).not.toHaveBeenCalled()
  })

  it.each([false, new Error("disconnect failed")])(
    "keeps the current project and panels when replacement fails: %s",
    async (result) => {
      useTabStore.getState().openNewConversationTab(1, "C:/one")
      opened.mockClear()
      if (result instanceof Error) disconnect.mockRejectedValueOnce(result)
      else disconnect.mockResolvedValueOnce(result)
      useTabStore.getState().openNewConversationTab(2, "C:/two")
      useTabStore.getState().consumeDraftRetargets()
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(opened).not.toHaveBeenCalled()
      expect(useTabStore.getState().rawTabs[0].folderId).toBe(1)
    }
  )

  it("notifies only after a project retarget actually succeeds", async () => {
    useTabStore.getState().openNewConversationTab(1, "C:/one")
    opened.mockClear()
    useTabStore.getState().openNewConversationTab(2, "C:/two")
    expect(opened).not.toHaveBeenCalled()
    useTabStore.getState().consumeDraftRetargets()
    await vi.waitFor(() => expect(opened).toHaveBeenCalledTimes(1))
    expect(useTabStore.getState().rawTabs[0].folderId).toBe(2)
  })

  it("does not collapse a different active conversation after late retarget", async () => {
    useTabStore.getState().openNewConversationTab(1, "C:/one")
    opened.mockClear()
    let finish!: (value: boolean) => void
    disconnect.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve
      })
    )
    useTabStore.getState().openNewConversationTab(2, "C:/two")
    useTabStore.getState().consumeDraftRetargets()
    useTabStore.getState().openTab(3, 99, "codex", true, "history")
    finish(true)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(opened).not.toHaveBeenCalled()
  })

  it("does not notify when folderless replacement is canceled", async () => {
    useTabStore.getState().openNewConversationTab(1, "C:/one")
    opened.mockClear()
    disconnect.mockResolvedValueOnce(false)
    useTabStore.getState().openChatModeTab()
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(opened).not.toHaveBeenCalled()
  })

  it("unsubscribes the layout when it unmounts", () => {
    unsubscribe()
    useTabStore.getState().openChatModeTab()
    expect(opened).not.toHaveBeenCalled()
  })
})
