import "@testing-library/jest-dom/vitest"

import { fireEvent, render, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({
  isDesktop: vi.fn(() => true),
  platform: {
    platform: "windows",
    isMac: false,
    isWindows: true,
    isLinux: false,
  },
  appWindow: {
    isMaximized: vi.fn(async () => false),
    onResized: vi.fn(async () => () => {}),
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
  },
}))

vi.mock("@/lib/platform", () => ({ isDesktop: mocks.isDesktop }))
vi.mock("@/hooks/use-platform", () => ({ usePlatform: () => mocks.platform }))
vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => mocks.appWindow,
}))

import { OverlayWindowControls } from "./overlay-window-controls"

describe("OverlayWindowControls", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mocks.isDesktop.mockReturnValue(true)
    Object.assign(mocks.platform, {
      platform: "windows",
      isMac: false,
      isWindows: true,
      isLinux: false,
    })
  })

  it("renders the three controls while a startup gate is blocking", () => {
    const { getAllByRole, getByRole } = render(
      <OverlayWindowControls visible />
    )

    expect(getAllByRole("button")).toHaveLength(3)
    expect(getByRole("button", { name: "minimizeWindow" })).toBeDefined()
    expect(getByRole("button", { name: "maximizeWindow" })).toBeDefined()
    expect(getByRole("button", { name: "closeWindow" })).toBeDefined()
  })

  it("renders nothing once the gate stops blocking", () => {
    const { container } = render(<OverlayWindowControls visible={false} />)
    expect(container).toBeEmptyDOMElement()
  })

  it("renders nothing on macOS, which keeps its native traffic lights", () => {
    Object.assign(mocks.platform, {
      platform: "macos",
      isMac: true,
      isWindows: false,
      isLinux: false,
    })

    const { container } = render(<OverlayWindowControls visible />)
    expect(container).toBeEmptyDOMElement()
  })

  it("renders nothing in the web build, which has no window to control", () => {
    mocks.isDesktop.mockReturnValue(false)

    const { container } = render(<OverlayWindowControls visible />)
    expect(container).toBeEmptyDOMElement()
  })

  it("drives the Tauri window from each control", async () => {
    const { getByRole } = render(<OverlayWindowControls visible />)

    // The window handle is resolved through a dynamic import, so the click
    // targets are only wired up once that settles.
    await waitFor(() => expect(mocks.appWindow.isMaximized).toHaveBeenCalled())

    fireEvent.click(getByRole("button", { name: "minimizeWindow" }))
    expect(mocks.appWindow.minimize).toHaveBeenCalledOnce()

    fireEvent.click(getByRole("button", { name: "maximizeWindow" }))
    expect(mocks.appWindow.toggleMaximize).toHaveBeenCalledOnce()

    fireEvent.click(getByRole("button", { name: "closeWindow" }))
    expect(mocks.appWindow.close).toHaveBeenCalledOnce()
  })

  it("stacks above the dialog and re-enables pointer events on the buttons", () => {
    // These two classes are the whole reason the component exists: a Radix modal
    // sets `pointer-events: none` on <body> and paints its overlay at z-50, so
    // without them the controls are either invisible or unclickable. jsdom loads
    // no Tailwind stylesheet, so the class names are the observable here.
    const { container } = render(<OverlayWindowControls visible />)
    const wrapper = container.firstElementChild

    expect(wrapper?.className).toContain("z-[110]")
    expect(wrapper?.className).toContain("[&_button]:pointer-events-auto")
  })
})
