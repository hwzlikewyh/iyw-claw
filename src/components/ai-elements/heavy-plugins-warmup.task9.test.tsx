import { act, render } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({ prefetchHeavyPlugins: vi.fn() }))

vi.mock("./streamdown-plugins", () => ({
  prefetchHeavyPlugins: mocks.prefetchHeavyPlugins,
}))

import { HeavyPluginsWarmup } from "./heavy-plugins-warmup"

afterEach(() => {
  mocks.prefetchHeavyPlugins.mockClear()
  vi.unstubAllGlobals()
})

describe("HeavyPluginsWarmup", () => {
  it("warms only the code engine on the first user interaction", () => {
    vi.stubGlobal(
      "requestIdleCallback",
      vi.fn(() => 1)
    )
    vi.stubGlobal("cancelIdleCallback", vi.fn())
    render(<HeavyPluginsWarmup />)

    act(() => {
      window.dispatchEvent(new Event("pointerdown"))
      window.dispatchEvent(new Event("keydown"))
    })

    expect(mocks.prefetchHeavyPlugins).toHaveBeenCalledTimes(1)
    expect(mocks.prefetchHeavyPlugins).toHaveBeenCalledWith(["code"])
  })
})
