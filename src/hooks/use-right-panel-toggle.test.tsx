import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { useRightPanelToggle } from "./use-right-panel-toggle"

const mocks = vi.hoisted(() => ({
  auxOpen: false,
  browserOpen: false,
  pageOpen: false,
  browserVisible: true,
  localDesktop: true,
  setOpen: vi.fn(),
  openTab: vi.fn(),
  openBrowser: vi.fn(async () => {}),
  closeBrowser: vi.fn(async () => {}),
  closePanel: vi.fn(),
}))

vi.mock("@/contexts/aux-panel-context", () => ({
  useAuxPanelContext: () => ({
    isOpen: mocks.auxOpen,
    setOpen: mocks.setOpen,
    openTab: mocks.openTab,
  }),
}))
vi.mock("@/contexts/browser-context", () => ({
  useBrowser: () => ({
    isOpen: mocks.browserOpen,
    openBrowser: mocks.openBrowser,
    closeBrowser: mocks.closeBrowser,
  }),
}))
vi.mock("@/contexts/interactive-page-context", () => ({
  useInteractivePage: () => ({
    isOpen: mocks.pageOpen,
    closePanel: mocks.closePanel,
  }),
}))
vi.mock("./use-browser-visibility", () => ({
  useBrowserVisibility: () => mocks.browserVisible,
}))
vi.mock("@/lib/platform", () => ({
  isLocalDesktop: () => mocks.localDesktop,
}))

beforeEach(() => {
  vi.clearAllMocks()
  Object.assign(mocks, {
    auxOpen: false,
    browserOpen: false,
    pageOpen: false,
    browserVisible: true,
    localDesktop: true,
  })
})

describe("title bar right sidebar toggle", () => {
  it.each([
    [true, true],
    [true, false],
    [false, true],
  ])(
    "opens overview with browser visible=%s and local desktop=%s",
    (visible, local) => {
      mocks.browserVisible = visible
      mocks.localDesktop = local
      const { result } = renderHook(useRightPanelToggle)
      expect(result.current.isOpen).toBe(false)
      expect(mocks.openTab).not.toHaveBeenCalled()
      act(() => result.current.toggle())
      expect(mocks.openTab).toHaveBeenCalledWith("home")
      expect(mocks.openBrowser).not.toHaveBeenCalled()
    }
  )

  it.each(["auxOpen", "browserOpen", "pageOpen"] as const)(
    "closes an existing %s without opening another page",
    (panel) => {
      mocks[panel] = true
      const { result } = renderHook(useRightPanelToggle)
      expect(result.current.isOpen).toBe(true)
      act(() => result.current.toggle())
      expect(mocks.closePanel).toHaveBeenCalledTimes(1)
      expect(mocks.setOpen).toHaveBeenCalledWith(false)
      expect(mocks.closeBrowser).toHaveBeenCalledTimes(1)
      expect(mocks.openTab).not.toHaveBeenCalled()
      expect(mocks.openBrowser).not.toHaveBeenCalled()
    }
  )

  it("returns to overview after closing another right panel", () => {
    mocks.browserOpen = true
    const { result, rerender } = renderHook(useRightPanelToggle)
    act(() => result.current.toggle())
    mocks.browserOpen = false
    rerender()
    act(() => result.current.toggle())
    expect(mocks.openTab).toHaveBeenCalledTimes(1)
    expect(mocks.openTab).toHaveBeenCalledWith("home")
    expect(mocks.openBrowser).not.toHaveBeenCalled()
  })
})
