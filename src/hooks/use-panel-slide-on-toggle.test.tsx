import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
  PANEL_SLIDE_FALLBACK_MS,
  usePanelSlideOnToggle,
} from "./use-panel-slide-on-toggle"

describe("usePanelSlideOnToggle", () => {
  const groupId = "test-panel-group"

  beforeEach(() => {
    vi.useFakeTimers()
    document.body.innerHTML = `
      <div data-panel-group-id="${groupId}">
        <div data-panel></div>
      </div>
    `
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it("does not animate the initial persisted-state restore", () => {
    const { rerender } = renderHook(
      ({ open, ready }) => usePanelSlideOnToggle(groupId, open, ready),
      { initialProps: { open: true, ready: false } }
    )

    rerender({ open: false, ready: true })

    const group = document.querySelector(`[data-panel-group-id="${groupId}"]`)
    expect(group?.classList.contains("panel-slide-animating")).toBe(false)
  })

  it("clears when a direct panel finishes its flex-grow transition", () => {
    const { rerender } = renderHook(
      ({ open, ready }) => usePanelSlideOnToggle(groupId, open, ready),
      { initialProps: { open: true, ready: true } }
    )

    rerender({ open: false, ready: true })
    const group = document.querySelector(`[data-panel-group-id="${groupId}"]`)
    expect(group?.classList.contains("panel-slide-animating")).toBe(true)

    const transitionEnd = new Event("transitionend", { bubbles: true })
    Object.defineProperty(transitionEnd, "propertyName", { value: "flex-grow" })
    document.querySelector("[data-panel]")?.dispatchEvent(transitionEnd)

    expect(group?.classList.contains("panel-slide-animating")).toBe(false)
  })

  it("uses a delayed fallback when no transitionend event arrives", () => {
    const { rerender } = renderHook(
      ({ open, ready }) => usePanelSlideOnToggle(groupId, open, ready),
      { initialProps: { open: true, ready: true } }
    )

    rerender({ open: false, ready: true })
    const group = document.querySelector(`[data-panel-group-id="${groupId}"]`)

    act(() => vi.advanceTimersByTime(PANEL_SLIDE_FALLBACK_MS - 1))
    expect(group?.classList.contains("panel-slide-animating")).toBe(true)

    act(() => vi.advanceTimersByTime(1))
    expect(group?.classList.contains("panel-slide-animating")).toBe(false)
  })

  it("restarts the cleanup window after a rapid second toggle", () => {
    const { rerender } = renderHook(
      ({ open, ready }) => usePanelSlideOnToggle(groupId, open, ready),
      { initialProps: { open: true, ready: true } }
    )

    rerender({ open: false, ready: true })
    act(() => vi.advanceTimersByTime(PANEL_SLIDE_FALLBACK_MS - 20))
    rerender({ open: true, ready: true })
    act(() => vi.advanceTimersByTime(20))

    const group = document.querySelector(`[data-panel-group-id="${groupId}"]`)
    expect(group?.classList.contains("panel-slide-animating")).toBe(true)

    act(() => vi.advanceTimersByTime(PANEL_SLIDE_FALLBACK_MS - 20))
    expect(group?.classList.contains("panel-slide-animating")).toBe(false)
  })
})
