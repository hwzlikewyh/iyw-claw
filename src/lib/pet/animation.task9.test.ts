import { describe, expect, it } from "vitest"

import {
  backgroundPositionFor,
  filmstripFrameCount,
  spriteBackgroundSize,
  spriteRowsFromHeight,
} from "./animation"

describe("variable-height pet geometry", () => {
  it("derives v1 and v2 row counts from natural height", () => {
    expect(spriteRowsFromHeight(1872)).toBe(9)
    expect(spriteRowsFromHeight(2288)).toBe(11)
    expect(spriteRowsFromHeight(null)).toBe(9)
  })

  it("scales and positions the same row against the actual grid", () => {
    expect(spriteBackgroundSize(9)).toBe("800% 900%")
    expect(spriteBackgroundSize(11)).toBe("800% 1100%")
    expect(backgroundPositionFor(5, 0, 11)).toBe("0% 50%")
  })

  it("counts frames in v1 and v2 marketplace filmstrips", () => {
    expect(filmstripFrameCount(5472, 104)).toBe(57)
    expect(filmstripFrameCount(7008, 104)).toBe(73)
  })
})
