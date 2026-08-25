import { describe, expect, it } from "vitest"

import { resolveArtifactConversationId } from "./artifact-conversation-id"

describe("resolveArtifactConversationId", () => {
  it("preserves an explicit unpersisted draft instead of using its runtime ID", () => {
    expect(resolveArtifactConversationId(-42, null)).toBeNull()
  })

  it("uses the persisted ID after a draft is created", () => {
    expect(resolveArtifactConversationId(-42, 152)).toBe(152)
  })

  it("keeps existing and embedded conversations backward compatible", () => {
    expect(resolveArtifactConversationId(151)).toBe(151)
  })
})
