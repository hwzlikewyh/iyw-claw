import { beforeEach, describe, expect, it } from "vitest"

import type { DbConversationSummary } from "@/lib/types"
import {
  resetAppWorkspaceStore,
  useAppWorkspaceStore,
} from "./app-workspace-store"

function summary(id: number): DbConversationSummary {
  return {
    id,
    folder_id: 1,
    title: null,
    title_locked: false,
    agent_type: "claude_code",
    status: "in_progress",
    kind: "regular",
    model: null,
    git_branch: null,
    external_id: null,
    message_count: id,
    child_count: 0,
    created_at: "2026-01-01T00:00:00.000Z",
    updated_at: "2026-01-01T00:00:00.000Z",
    pinned_at: null,
    parent_id: null,
    parent_tool_use_id: null,
    delegation_call_id: null,
  }
}

beforeEach(() => resetAppWorkspaceStore())

describe("updateConversationLocal stats stability", () => {
  it("reuses stats for a status-only patch while updating the row", () => {
    const store = useAppWorkspaceStore.getState()
    store.applyConversationUpsert(summary(1))
    store.applyConversationUpsert(summary(2))
    const before = useAppWorkspaceStore.getState()

    before.updateConversationLocal(1, { status: "pending_review" })

    const after = useAppWorkspaceStore.getState()
    expect(after.stats).toBe(before.stats)
    expect(after.conversations).not.toBe(before.conversations)
    expect(after.conversations.find((row) => row.id === 1)?.status).toBe(
      "pending_review"
    )
  })
})
