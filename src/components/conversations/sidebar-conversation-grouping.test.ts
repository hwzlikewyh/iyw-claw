import { describe, expect, it } from "vitest"

import { mergeChildrenById } from "./sidebar-conversation-grouping"
import type { DbConversationSummary } from "@/lib/types"

const child = (id: number, createdAt: string): DbConversationSummary => ({
  id,
  folder_id: 1,
  title: `Child ${id}`,
  title_locked: false,
  agent_type: "codex",
  status: "in_progress",
  kind: "delegate",
  model: null,
  git_branch: null,
  external_id: null,
  message_count: 0,
  child_count: 0,
  created_at: createdAt,
  updated_at: createdAt,
  pinned_at: null,
  parent_id: 1,
  parent_tool_use_id: `tool-${id}`,
  delegation_call_id: `call-${id}`,
})

describe("mergeChildrenById", () => {
  it("orders delegation children newest-first with id as a tie-break", () => {
    const older = child(2, "2026-07-01T10:00:00.000Z")
    const newerLowId = child(3, "2026-07-01T11:00:00.000Z")
    const newerHighId = child(4, "2026-07-01T11:00:00.000Z")

    expect(
      mergeChildrenById([older, newerLowId], [newerHighId]).map(
        (item) => item.id
      )
    ).toEqual([4, 3, 2])
  })
})
