import { describe, expect, it } from "vitest"
import {
  fitMarketPage,
  marketPageSize,
  persistMarketPage,
} from "./market-pagination"

describe("viewport-driven market pagination", () => {
  it.each([
    [640, 3, 6],
    [890, 3, 12],
    [460, 2, 4],
    [500, 1, 2],
    [120, 1, 1],
  ])(
    "fits complete rows in %s pixels across %s columns",
    (availableHeight, columns, count) => {
      expect(
        fitMarketPage({ availableHeight, columns, cardHeight: 208, rowGap: 16 })
      ).toBe(count)
    }
  )
  it("uses actual card height instead of the old six-item default", () => {
    expect(
      fitMarketPage({
        availableHeight: 700,
        columns: 3,
        cardHeight: 220,
        rowGap: 16,
      })
    ).toBe(9)
  })
  it("retains adaptive counts that are not manual presets", () => {
    expect(marketPageSize(9)).toBe(9)
    expect(marketPageSize(4)).toBe(4)
    expect(marketPageSize(12)).toBe(12)
    expect(marketPageSize(-1)).toBe(6)
  })
  it("distinguishes manual six from automatic mode in the URL", () => {
    window.history.replaceState(null, "", "/")
    persistMarketPage(1, 6, false)
    expect(new URLSearchParams(window.location.search).get("pageSize")).toBe(
      "6"
    )
    persistMarketPage(1, 9, true)
    expect(new URLSearchParams(window.location.search).has("pageSize")).toBe(
      false
    )
  })
})
