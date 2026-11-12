import { fireEvent, render } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { useMarketPageFocus } from "./use-market-page-focus"

const scrollTo = vi.fn()
function Page({
  page,
  loading = false,
  error = "",
  enabled = true,
}: {
  page: number
  loading?: boolean
  error?: string
  enabled?: boolean
}) {
  const root = useMarketPageFocus({ page, loading, error, enabled })
  return (
    <div data-market-scroll>
      <div ref={root}>
        <input aria-label="Search" />
        <select aria-label="Page size">
          <option>12</option>
        </select>
        {!loading &&
          (error ? (
            <p role="alert">{error}</p>
          ) : (
            <button data-market-card-title>Page {page}</button>
          ))}
        <button>Next</button>
      </div>
    </div>
  )
}
beforeEach(() => {
  vi.clearAllMocks()
  Object.defineProperty(HTMLElement.prototype, "scrollTo", {
    configurable: true,
    value: scrollTo,
  })
  vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue([
    {},
  ] as unknown as DOMRectList)
})
afterEach(() => vi.restoreAllMocks())
describe("market page navigation focus", () => {
  it("waits for a completed new page before focusing its first card", () => {
    const view = render(<Page page={1} />)
    expect(scrollTo).not.toHaveBeenCalled()
    view.getByText("Next").focus()
    view.rerender(<Page page={2} loading />)
    expect(scrollTo).not.toHaveBeenCalled()
    view.rerender(<Page page={2} />)
    expect(scrollTo).toHaveBeenCalledWith({ top: 0 })
    expect(document.activeElement).toBe(view.getByText("Page 2"))
    view.unmount()
  })
  it("focuses the error when the requested page fails", () => {
    const view = render(<Page page={1} />)
    view.getByText("Next").focus()
    view.rerender(<Page page={2} error="Page failed" />)
    expect(document.activeElement).toBe(view.getByRole("alert"))
    view.unmount()
  })
  it.each(["Search", "Page size"])("keeps focus in the %s filter", (label) => {
    const view = render(<Page page={2} />)
    const input = view.getByLabelText(label)
    input.focus()
    fireEvent.change(input, {
      target: { value: label === "Search" ? "PDF" : "12" },
    })
    view.rerender(<Page page={1} />)
    expect(document.activeElement).toBe(input)
    expect(scrollTo).not.toHaveBeenCalled()
    view.unmount()
  })
  it("defers navigation while that market is hidden", () => {
    const view = render(<Page page={1} />)
    view.rerender(<Page page={2} enabled={false} />)
    expect(scrollTo).not.toHaveBeenCalled()
    view.rerender(<Page page={2} />)
    expect(document.activeElement).toBe(view.getByText("Page 2"))
    view.unmount()
  })
})
