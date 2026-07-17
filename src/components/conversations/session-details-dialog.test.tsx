import { describe, expect, it } from "vitest"

import { resolveSessionDurationMs } from "./session-details-dialog"
import type { DbConversationSummary, SessionStats } from "@/lib/types"

const summary = (
  overrides: Partial<DbConversationSummary> = {}
): DbConversationSummary => ({
  id: 1,
  folder_id: 1,
  title: "Session",
  title_locked: false,
  agent_type: "codex",
  status: "completed",
  kind: "regular",
  model: null,
  git_branch: null,
  external_id: null,
  message_count: 0,
  child_count: 0,
  created_at: "2026-06-10T10:00:00.000Z",
  updated_at: "2026-06-10T10:02:30.000Z",
  pinned_at: null,
  parent_id: null,
  parent_tool_use_id: null,
  delegation_call_id: null,
  ...overrides,
})

describe("resolveSessionDurationMs", () => {
  it("uses completed timestamps when parsed duration is missing", () => {
    expect(resolveSessionDurationMs(summary(), null)).toBe(150_000)
  })

  it("does not derive a live duration from updated_at", () => {
    expect(
      resolveSessionDurationMs(summary({ status: "in_progress" }), null)
    ).toBe(0)
  })

  it("prefers the parsed duration", () => {
    const stats = { total_duration_ms: 5_000 } as SessionStats
    expect(resolveSessionDurationMs(summary(), stats)).toBe(5_000)
  })
})
