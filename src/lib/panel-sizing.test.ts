import { describe, expect, it } from "vitest"

import { scalePanelPixels, unscalePanelPixels } from "./panel-sizing"

describe("scalePanelPixels", () => {
  it.each([
    [52, 80, 41.6],
    [52, 100, 52],
    [52, 150, 78],
  ])("scales %dpx at %d%% to %dpx", (pixels, zoom, expected) => {
    expect(scalePanelPixels(pixels, zoom)).toBe(expected)
  })

  it.each([
    [160, 80, 200],
    [200, 100, 200],
    [300, 150, 200],
  ])("restores %dpx at %d%% to %dpx", (pixels, zoom, expected) => {
    expect(unscalePanelPixels(pixels, zoom)).toBe(expected)
  })
})
