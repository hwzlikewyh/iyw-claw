import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useProcessAutoFollow } from "./use-process-auto-follow"

function AutoFollowHarness({ version }: { version: number }) {
  const { handleScroll, isFollowing, scrollToLatest, viewportRef } =
    useProcessAutoFollow(version, true)
  return (
    <>
      <div data-testid="viewport" ref={viewportRef} onScroll={handleScroll}>
        <div />
      </div>
      <button type="button" onClick={scrollToLatest}>
        {isFollowing ? "following" : "paused"}
      </button>
    </>
  )
}

describe("useProcessAutoFollow", () => {
  let resizeCallback!: ResizeObserverCallback

  beforeEach(() => {
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0)
      return 1
    })
    vi.stubGlobal("cancelAnimationFrame", vi.fn())
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: ResizeObserverCallback) {
          resizeCallback = callback
        }
        observe() {}
        disconnect() {}
        unobserve() {}
      }
    )
  })

  it("follows new content until the user scrolls away and resumes on demand", () => {
    const view = render(<AutoFollowHarness version={0} />)
    const viewport = screen.getByTestId("viewport")
    let scrollHeight = 200
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, get: () => 50 },
      scrollHeight: { configurable: true, get: () => scrollHeight },
      scrollTop: { configurable: true, value: 0, writable: true },
    })

    view.rerender(<AutoFollowHarness version={1} />)
    expect(viewport.scrollTop).toBe(200)

    viewport.scrollTop = 40
    fireEvent.scroll(viewport)
    expect(screen.getByRole("button").textContent).toBe("paused")

    scrollHeight = 300
    view.rerender(<AutoFollowHarness version={2} />)
    expect(viewport.scrollTop).toBe(40)

    fireEvent.click(screen.getByRole("button"))
    expect(viewport.scrollTop).toBe(300)
    expect(screen.getByRole("button").textContent).toBe("following")
  })

  it("follows content that grows after its first layout", () => {
    render(<AutoFollowHarness version={0} />)
    const viewport = screen.getByTestId("viewport")
    let scrollHeight = 200
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, get: () => 50 },
      scrollHeight: { configurable: true, get: () => scrollHeight },
      scrollTop: { configurable: true, value: 0, writable: true },
    })

    scrollHeight = 320
    resizeCallback([], {} as ResizeObserver)

    expect(viewport.scrollTop).toBe(320)
  })
})
