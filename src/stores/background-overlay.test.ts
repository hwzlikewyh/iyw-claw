import { beforeEach, describe, expect, it, vi } from "vitest"

import { parseBackgroundTaskMarker } from "@/lib/background-agent"
import type { DbConversationDetail, MessageTurn } from "@/lib/types"
import {
  resetConversationRuntimeStore,
  selectTimelineTurns,
  useConversationRuntimeStore,
} from "@/stores/conversation-runtime-store"

vi.mock("@/lib/api", () => ({ getFolderConversation: vi.fn() }))

const { getFolderConversation } = await import("@/lib/api")
const getDetail = vi.mocked(getFolderConversation)

function turn(id: string, text: string, timestamp: string): MessageTurn {
  return {
    id,
    role: "assistant",
    blocks: [{ type: "text", text }],
    timestamp,
  }
}

function detail(
  overrides: Partial<DbConversationDetail> = {}
): DbConversationDetail {
  return {
    summary: {
      id: 7,
      folder_id: 1,
      title: "test",
      title_locked: false,
      agent_type: "claude_code",
      status: "in_progress",
      kind: "regular",
      model: null,
      git_branch: null,
      external_id: "session-7",
      message_count: 0,
      child_count: 0,
      created_at: "2026-07-16T09:00:00.000Z",
      updated_at: "2026-07-16T09:00:00.000Z",
      pinned_at: null,
    },
    turns: [],
    session_stats: null,
    ...overrides,
  }
}

function launchTurn(toolUseId = "toolu-1"): MessageTurn {
  return {
    id: "launch-turn",
    role: "assistant",
    timestamp: "2026-07-16T10:00:00.000Z",
    blocks: [
      {
        type: "tool_use",
        tool_use_id: toolUseId,
        tool_name: "Agent",
        input_preview: null,
      },
      {
        type: "tool_result",
        tool_use_id: toolUseId,
        output_preview: "Async agent launched successfully.",
        is_error: false,
      },
    ],
  }
}

const settlement = {
  toolUseId: "toolu-1",
  taskId: "agent-1",
  status: "completed",
  summary: "Done",
  result: "Build OK",
}

function actions() {
  return useConversationRuntimeStore.getState().actions
}

function session(id = 7) {
  return useConversationRuntimeStore.getState().byConversationId.get(id)
}

async function flush() {
  await Promise.resolve()
  await Promise.resolve()
}

beforeEach(() => {
  resetConversationRuntimeStore()
  getDetail.mockReset()
  getDetail.mockImplementation(() => new Promise(() => {}))
})

describe("background overlay", () => {
  it("upserts turns and retires only watermarks covered by detail", async () => {
    actions().applyBackgroundActivity(
      7,
      [turn("bg-1", "one", "2026-07-16T10:00:00.000Z")],
      100
    )
    actions().applyBackgroundActivity(
      7,
      [
        turn("bg-1", "one grown", "2026-07-16T10:00:00.000Z"),
        turn("bg-2", "two", "2026-07-16T10:02:00.000Z"),
      ],
      300
    )
    expect(session()?.backgroundTurns.map((entry) => entry.turn.id)).toEqual([
      "bg-1",
      "bg-2",
    ])

    getDetail.mockResolvedValueOnce(detail({ transcript_watermark: 200 }))
    actions().refetchDetail(7)
    await flush()
    expect(session()?.backgroundTurns.map((entry) => entry.turn.id)).toEqual([
      "bg-1",
      "bg-2",
    ])

    getDetail.mockResolvedValueOnce(detail({ transcript_watermark: 300 }))
    actions().refetchDetail(7)
    await flush()
    expect(session()?.backgroundTurns).toEqual([])
  })

  it("interleaves local and background turns by timestamp", () => {
    actions().applyBackgroundActivity(
      7,
      [turn("bg-early", "early", "2026-07-16T10:00:00.000Z")],
      100
    )
    actions().appendOptimisticTurn(
      7,
      {
        id: "local-user",
        role: "user",
        blocks: [{ type: "text", text: "hello" }],
        timestamp: "2026-07-16T10:01:00.000Z",
      },
      "token-1"
    )
    actions().completeTurn(7, null)
    actions().applyBackgroundActivity(
      7,
      [turn("bg-late", "late", "2026-07-16T10:02:00.000Z")],
      200
    )
    const ids = selectTimelineTurns(
      useConversationRuntimeStore.getState(),
      7
    ).map((entry) => entry.turn.id)
    expect(ids.indexOf("bg-early")).toBeLessThan(ids.indexOf("local-user"))
    expect(ids.indexOf("local-user")).toBeLessThan(ids.indexOf("bg-late"))
  })
})

describe("background settlement", () => {
  it("patches an existing launch card without refetching", () => {
    actions().appendOptimisticTurn(7, launchTurn(), "token-1")
    actions().completeTurn(7, null)
    actions().resolveBackgroundTask(7, settlement)

    const result = session()!
      .localTurns.flatMap((item) => item.blocks)
      .find((block) => block.type === "tool_result")
    expect(
      result?.type === "tool_result"
        ? parseBackgroundTaskMarker(result.output_preview)
        : null
    ).toMatchObject({
      taskId: "agent-1",
      status: "completed",
      result: "Build OK",
    })
    expect(getDetail).not.toHaveBeenCalled()
  })

  it("queues a settle before promotion and drains it on complete", () => {
    actions().appendOptimisticTurn(
      7,
      {
        id: "user-1",
        role: "user",
        blocks: [{ type: "text", text: "run" }],
        timestamp: "2026-07-16T09:59:00.000Z",
      },
      "token-1"
    )
    actions().resolveBackgroundTask(7, settlement)
    expect(session()?.pendingBackgroundSettlements).toHaveLength(1)

    actions().appendOptimisticTurn(7, launchTurn(), "token-1")
    actions().completeTurn(7, null)
    expect(session()?.pendingBackgroundSettlements).toEqual([])
  })
})
