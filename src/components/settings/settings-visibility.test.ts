import { describe, expect, it } from "vitest"

import { SETTINGS_NAV_ITEMS } from "./settings-shell"

describe("settings visibility", () => {
  it("hides the runtime logs entry from settings navigation", () => {
    expect(SETTINGS_NAV_ITEMS.map((item) => item.href)).not.toContain(
      "/settings/logs"
    )
  })
})
