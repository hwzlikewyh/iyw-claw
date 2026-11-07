import { act, renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { useTabStore } from "@/contexts/tab-context"
import { readPluginPanels, type PluginPanelChoice } from "@/lib/plugin-panel"
import { notifyPluginPreparation } from "@/lib/plugin-preparation-events"
import { usePluginPanelOpenIntent } from "./use-plugin-panel-open-intent"

const mocks = vi.hoisted(() => ({
  bind: vi.fn(),
  open: vi.fn<(app: PluginPanelChoice, guard: () => boolean) => Promise<void>>(
    async () => {}
  ),
  showPreparation: vi.fn(),
  showConversation: vi.fn(),
  transport: { onReconnect: vi.fn(() => () => {}) },
  isOpen: true,
  onNew: null as null | (() => void),
}))
vi.mock("@/contexts/tab-context", async () => {
  const { create } = await import("zustand")
  return {
    useTabStore: create(() => ({
      activeTabId: "draft",
      rawTabs: [{ id: "draft", conversationId: 42 }],
    })),
  }
})
vi.mock("@/stores/tab-store", () => ({
  onNewConversationOpened: (listener: () => void) => {
    mocks.onNew = listener
    return () => {
      mocks.onNew = null
    }
  },
}))
vi.mock("@/lib/transport", () => ({ getTransport: () => mocks.transport }))
vi.mock("@/lib/plugin-panel", () => ({ readPluginPanels: vi.fn() }))
vi.mock("./use-plugin-panel-choices", () => ({ pluginPanelError: String }))
vi.mock("./use-plugin-market-navigation", () => ({
  usePluginMarketNavigation: () => ({ openPlugin: mocks.showPreparation }),
}))
vi.mock("@/contexts/workbench-route-context", () => ({
  useWorkbenchRoute: () => ({ openConversations: mocks.showConversation }),
}))
vi.mock("@/contexts/aux-panel-context", () => ({
  useAuxPanelContext: () => ({ isOpen: mocks.isOpen }),
}))
vi.mock("@/components/skills/market/use-plugin-workspace", () => ({
  usePluginWorkspace: () => ({ bind: mocks.bind, busy: false, error: "" }),
}))

const app: PluginPanelChoice = {
  pluginId: "123",
  pluginSlug: "iyw-pdfcraft",
  version: "0.1.0",
  appKey: "editor",
  label: "PdfCraft",
  state: "context_required",
}
const available = { ...app, state: "available" as const }
const mounts: Array<() => void> = []
function mount(id: number | null) {
  const value = renderHook(
    ({ conversationId }) =>
      usePluginPanelOpenIntent({
        conversationId,
        transport: mocks.transport as never,
        open: mocks.open,
      }),
    { initialProps: { conversationId: id } }
  )
  mounts.push(value.unmount)
  return value
}
beforeEach(() => {
  vi.clearAllMocks()
  useTabStore.setState({ activeTabId: "draft" })
  mocks.bind.mockResolvedValue(42)
  mocks.open.mockResolvedValue(undefined)
  mocks.isOpen = true
  vi.mocked(readPluginPanels).mockResolvedValue([available])
})
afterEach(() => {
  for (const unmount of mounts.splice(0)) unmount()
})

describe("plugin open intent", () => {
  it("binds once, waits for the bound render, and opens the fresh app", async () => {
    const view = mount(null)
    await act(async () => {
      await view.result.current.open(app)
    })
    expect(mocks.bind).toHaveBeenCalledTimes(1)
    expect(mocks.open).not.toHaveBeenCalled()
    view.rerender({ conversationId: 42 })
    await waitFor(() => expect(mocks.open).toHaveBeenCalledTimes(1))
    expect(readPluginPanels).toHaveBeenCalledWith(42, mocks.transport)
    expect(mocks.open.mock.calls[0][0]).toEqual(available)
  })

  it("locks immediately against double clicks before binding finishes", async () => {
    let resolve!: (id: number) => void
    mocks.bind.mockReturnValue(
      new Promise((done) => {
        resolve = done
      })
    )
    const view = mount(null)
    let first!: Promise<void>
    act(() => {
      first = view.result.current.open(app)
      void view.result.current.open(app)
    })
    expect(mocks.bind).toHaveBeenCalledTimes(1)
    await act(async () => {
      resolve(42)
      await first
    })
    view.rerender({ conversationId: 42 })
    await waitFor(() => expect(mocks.open).toHaveBeenCalledTimes(1))
  })

  it("cancels when switching away and back while binding", async () => {
    let resolve!: (id: number) => void
    mocks.bind.mockReturnValue(
      new Promise((done) => {
        resolve = done
      })
    )
    const view = mount(null)
    let first!: Promise<void>
    act(() => {
      first = view.result.current.open(app)
    })
    act(() => {
      useTabStore.setState({ activeTabId: "other" })
      useTabStore.setState({ activeTabId: "draft" })
    })
    await act(async () => {
      resolve(42)
      await first
    })
    view.rerender({ conversationId: 42 })
    expect(mocks.open).not.toHaveBeenCalled()
  })

  it("keeps the intent across preparation and rechecks after approval", async () => {
    vi.mocked(readPluginPanels)
      .mockResolvedValueOnce([{ ...app, state: "permission_required" }])
      .mockResolvedValue([available])
    const view = mount(42)
    await act(async () => {
      await view.result.current.open(app)
    })
    await waitFor(() =>
      expect(mocks.showPreparation).toHaveBeenCalledWith(app.pluginSlug)
    )
    mocks.isOpen = false
    view.rerender({ conversationId: 42 })
    act(() =>
      notifyPluginPreparation({ pluginId: app.pluginId, conversationId: 42 })
    )
    await waitFor(() => expect(mocks.open).toHaveBeenCalledTimes(1))
    expect(mocks.showConversation).toHaveBeenCalledTimes(1)
  })

  it("does not resume an explicitly canceled preparation", async () => {
    vi.mocked(readPluginPanels).mockResolvedValue([
      { ...app, state: "permission_required" },
    ])
    const view = mount(42)
    await act(async () => {
      await view.result.current.open(app)
    })
    await waitFor(() => expect(mocks.showPreparation).toHaveBeenCalledTimes(1))
    act(() =>
      notifyPluginPreparation({
        pluginId: app.pluginId,
        conversationId: 42,
        canceled: true,
      })
    )
    vi.mocked(readPluginPanels).mockResolvedValue([available])
    act(() =>
      notifyPluginPreparation({ pluginId: app.pluginId, conversationId: 42 })
    )
    expect(mocks.open).not.toHaveBeenCalled()
  })

  it("drops a catalog response after the original conversation closes", async () => {
    let resolve!: (apps: PluginPanelChoice[]) => void
    vi.mocked(readPluginPanels).mockReturnValue(
      new Promise((done) => {
        resolve = done
      })
    )
    const view = mount(42)
    await act(async () => {
      await view.result.current.open(available)
    })
    act(() => useTabStore.setState({ activeTabId: null }))
    await act(async () => {
      resolve([available])
    })
    expect(mocks.open).not.toHaveBeenCalled()
  })

  it("passes a live cancellation guard to an in-flight launch", async () => {
    let finish!: () => void
    mocks.open.mockImplementation(
      () =>
        new Promise<void>((done) => {
          finish = done
        })
    )
    const view = mount(42)
    await act(async () => {
      await view.result.current.open(available)
    })
    await waitFor(() => expect(mocks.open).toHaveBeenCalledTimes(1))
    const guard = mocks.open.mock.calls[0][1] as () => boolean
    expect(guard()).toBe(true)
    act(() => mocks.onNew?.())
    expect(guard()).toBe(false)
    await act(async () => {
      finish()
    })
  })

  it("reports missing apps and allows a real retry", async () => {
    vi.mocked(readPluginPanels).mockResolvedValueOnce([])
    const view = mount(42)
    await act(async () => {
      await view.result.current.open(available)
    })
    await waitFor(() =>
      expect(view.result.current.error).toContain("no longer available")
    )
    await act(async () => {
      await view.result.current.open(available)
    })
    await waitFor(() => expect(mocks.open).toHaveBeenCalledTimes(1))
  })
})
