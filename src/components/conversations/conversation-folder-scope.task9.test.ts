import { describe, expect, it } from "vitest"

import { resolveConversationFolderScope } from "./conversation-folder-scope"

describe("resolveConversationFolderScope", () => {
  it("includes direct worktree children and excludes unrelated folders", () => {
    expect(
      resolveConversationFolderScope(10, [
        { id: 11, parent_id: 10 },
        { id: 12, parent_id: 10 },
        { id: 20, parent_id: null },
        { id: 21, parent_id: 20 },
      ])
    ).toEqual([10, 11, 12])
  })
})
