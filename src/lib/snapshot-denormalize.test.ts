import { describe, expect, it } from "vitest"

import { denormalizeSnapshot } from "@/lib/snapshot-denormalize"
import type { LiveSessionSnapshot } from "@/lib/types"

const snapshot = (
  overrides: Partial<LiveSessionSnapshot> = {}
): LiveSessionSnapshot =>
  ({
    connection_id: "connection-1",
    conversation_id: 1,
    folder_id: 1,
    status: "connected",
    external_id: null,
    live_message: null,
    active_tool_calls: [],
    pending_permission: null,
    modes: null,
    current_mode: null,
    config_options: null,
    prompt_capabilities: null,
    usage: null,
    fork_supported: false,
    available_commands: [],
    selectors_ready: true,
    event_seq: 4,
    ...overrides,
  }) as LiveSessionSnapshot

describe("denormalizeSnapshot last_error", () => {
  it("recovers and trims the latest runtime error", () => {
    expect(
      denormalizeSnapshot(
        snapshot({
          last_error: {
            message: " ACP protocol error: Forbidden ",
            code: "forbidden",
          },
        })
      ).lastError
    ).toBe("ACP protocol error: Forbidden")
  })

  it("defaults to null for older snapshots", () => {
    expect(denormalizeSnapshot(snapshot()).lastError).toBeNull()
  })
})
