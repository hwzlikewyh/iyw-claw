import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { usePluginPanelRetry } from "./use-plugin-panel-retry"
import { readPluginPanels } from "@/lib/plugin-panel"
import { requestPluginApplicationOpen } from "@/lib/plugin-preparation-events"
import { useTabStore } from "@/contexts/tab-context"

const mocks = vi.hoisted(() => ({
  transport: { call: vi.fn(async () => {}) },
  onNew: null as null | (() => void),
}))
vi.mock("@/contexts/tab-context", async () => {
  const { create } = await import("zustand")
  return {
    useTabStore: create(() => ({
      activeTabId: "tab",
      tabs: [{ id: "tab", conversationId: 42 }],
    })),
  }
})
vi.mock("@/stores/tab-store", () => ({
  onNewConversationOpened: (callback: () => void) => {
    mocks.onNew = callback
    return () => {
      mocks.onNew = null
    }
  },
}))
vi.mock("@/lib/transport", () => ({
  getTransport: () => mocks.transport,
  onTransportChange: () => () => {},
}))
vi.mock("@/lib/plugin-panel", () => ({ readPluginPanels: vi.fn() }))
vi.mock("@/lib/plugin-preparation-events", () => ({
  requestPluginApplicationOpen: vi.fn(() => true),
}))
const tab = {
  id: "instance",
  conversationId: 42,
  pluginSlug: "iyw-pdfcraft",
  appKey: "editor",
  label: "PDF",
  response: {},
} as never
const app = {
  pluginId: "1",
  pluginSlug: "iyw-pdfcraft",
  appKey: "editor",
  version: "0.1.1",
  label: "PDF",
  state: "available",
} as const
beforeEach(() => {
  vi.clearAllMocks()
  useTabStore.setState({ activeTabId: "tab" })
  vi.mocked(readPluginPanels).mockResolvedValue([app])
})
describe("failed plugin retry", () => {
  it("closes the old instance, tears it down, and rechecks before reopening", async () => {
    const close = vi.fn(async () => true),
      onError = vi.fn()
    const { result, unmount } = renderHook(() =>
      usePluginPanelRetry({
        tabs: [tab],
        close,
        transport: mocks.transport as never,
        onError,
      })
    )
    await act(async () => {
      await result.current("instance")
    })
    expect(close).toHaveBeenCalledWith("instance")
    expect(mocks.transport.call).toHaveBeenCalledWith("plugin_app_teardown", {
      request: { instanceId: "instance", conversationId: 42 },
    })
    expect(readPluginPanels).toHaveBeenCalledWith(42, mocks.transport)
    expect(requestPluginApplicationOpen).toHaveBeenCalledWith(app)
    expect(onError).not.toHaveBeenCalled()
    unmount()
  })
  it("keeps the page when its close protocol refuses teardown", async () => {
    const { result, unmount } = renderHook(() =>
      usePluginPanelRetry({
        tabs: [tab],
        close: async () => false,
        transport: mocks.transport as never,
        onError: vi.fn(),
      })
    )
    await act(async () => {
      await result.current("instance")
    })
    expect(mocks.transport.call).not.toHaveBeenCalled()
    expect(requestPluginApplicationOpen).not.toHaveBeenCalled()
    unmount()
  })
  it("does not inspect the old conversation after switching during close", async () => {
    const close = vi.fn(async () => {
      useTabStore.setState({ activeTabId: "other" })
      return true
    })
    const { result, unmount } = renderHook(() =>
      usePluginPanelRetry({
        tabs: [tab],
        close,
        transport: mocks.transport as never,
        onError: vi.fn(),
      })
    )
    await act(async () => {
      await result.current("instance")
    })
    expect(readPluginPanels).not.toHaveBeenCalled()
    expect(requestPluginApplicationOpen).not.toHaveBeenCalled()
    unmount()
  })
  it("does not reopen into another conversation after an awaited close", async () => {
    let finish!: (value: boolean) => void
    const close = () =>
      new Promise<boolean>((resolve) => {
        finish = resolve
      })
    const { result, unmount } = renderHook(() =>
      usePluginPanelRetry({
        tabs: [tab],
        close,
        transport: mocks.transport as never,
        onError: vi.fn(),
      })
    )
    let request!: Promise<void>
    act(() => {
      request = result.current("instance")
    })
    useTabStore.setState({ activeTabId: "other" })
    await act(async () => {
      finish(true)
      await request
    })
    expect(requestPluginApplicationOpen).not.toHaveBeenCalled()
    unmount()
  })
  it.each(["switch-back", "new-conversation", "unmount"])(
    "cancels the pending retry after %s",
    async (reason) => {
      let finish!: (value: boolean) => void
      const { result, unmount } = renderHook(() =>
        usePluginPanelRetry({
          tabs: [tab],
          close: () =>
            new Promise<boolean>((resolve) => {
              finish = resolve
            }),
          transport: mocks.transport as never,
          onError: vi.fn(),
        })
      )
      let request!: Promise<void>
      act(() => {
        request = result.current("instance")
      })
      act(() => {
        if (reason === "switch-back") {
          useTabStore.setState({ activeTabId: "other" })
          useTabStore.setState({ activeTabId: "tab" })
        } else if (reason === "new-conversation") mocks.onNew?.()
        else unmount()
      })
      await act(async () => {
        finish(true)
        await request
      })
      expect(readPluginPanels).not.toHaveBeenCalled()
      expect(requestPluginApplicationOpen).not.toHaveBeenCalled()
      if (reason !== "unmount") unmount()
    }
  )
})
