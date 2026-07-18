import { describe, expect, it } from "vitest"

import { orderSessionSelectors } from "@/lib/session-selector-order"
import type { SessionConfigOptionInfo } from "@/lib/types"

function option(
  id: string,
  category: string | null = null
): SessionConfigOptionInfo {
  return {
    id,
    name: id,
    category,
    kind: {
      type: "select",
      current_value: "value",
      options: [{ value: "value", name: "Value" }],
      groups: [],
    },
  }
}

describe("session selector order", () => {
  it("places mode first, model second, then preserves other settings", () => {
    const ordered = orderSessionSelectors(true, [
      option("reasoning_effort", "thought_level"),
      option("web_search"),
      option("model", "model"),
      option("permission"),
    ])

    expect(
      ordered.map((item) =>
        item.kind === "mode" ? "mode" : `config:${item.option.id}`
      )
    ).toEqual([
      "mode",
      "config:model",
      "config:reasoning_effort",
      "config:web_search",
      "config:permission",
    ])
  })

  it("still places model first when no mode is available", () => {
    const ordered = orderSessionSelectors(false, [
      option("reasoning_effort"),
      option("model"),
    ])

    expect(
      ordered.map((item) =>
        item.kind === "mode" ? "mode" : `config:${item.option.id}`
      )
    ).toEqual(["config:model", "config:reasoning_effort"])
  })
})
