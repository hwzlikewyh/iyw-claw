import { describe, expect, it } from "vitest"

import { buildManagedPiConfig, PI_RESERVED_ENV_KEYS } from "./pi-config-panel"

describe("managed Pi runtime settings", () => {
  it("keeps only workspace trust user-configurable", () => {
    expect(PI_RESERVED_ENV_KEYS).toEqual(["PI_ACP_TRUST_WORKSPACE"])
  })

  it("always saves the iyw-claw provider", () => {
    expect(buildManagedPiConfig("  gpt-5.6  ", "high")).toEqual({
      provider: "iyw-claw",
      model: "gpt-5.6",
      thinkingLevel: "high",
    })
  })
})
