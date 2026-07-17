import { afterEach, describe, expect, it } from "vitest"
import {
  resetConversationRuntimeStore,
  useConversationRuntimeStore,
  type ConversationRuntimeSession,
} from "@/stores/conversation-runtime-store"
import type { DbConversationDetail, MessageTurn } from "@/lib/types"

const CONVERSATION_ID = 4242

function turn(id: string, role: "user" | "assistant"): MessageTurn {
  return {
    id,
    role,
    blocks: [],
    timestamp: "2026-01-01T00:00:00Z",
  }
}

function seedDetail(turns: MessageTurn[], inFlightUserTurnId?: string) {
  const detail = {
    summary: {
      id: CONVERSATION_ID,
      title: null,
      agent_type: "codex",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    },
    turns,
    in_flight_user_turn_id: inFlightUserTurnId ?? null,
  } as unknown as DbConversationDetail

  const session = {
    conversationId: CONVERSATION_ID,
    externalId: null,
    detail,
    detailLoading: false,
    detailError: null,
    acpLoadError: null,
    localTurns: [],
    optimisticTurns: [],
    liveMessage: null,
    syncState: "idle",
    activeTurnToken: null,
    liveOwnsActiveTurn: false,
    delegationKickoffText: null,
    sessionStats: null,
    historyAssistantBaseline: null,
    pendingCleanup: false,
  } as unknown as ConversationRuntimeSession

  useConversationRuntimeStore.setState((state) => ({
    byConversationId: new Map(state.byConversationId).set(
      CONVERSATION_ID,
      session
    ),
  }))
}

function baseline(): number | null {
  return (
    useConversationRuntimeStore.getState().byConversationId.get(CONVERSATION_ID)
      ?.historyAssistantBaseline ?? null
  )
}

describe("history assistant baseline", () => {
  afterEach(() => resetConversationRuntimeStore())

  it("captures settled history on the first owner prompt", () => {
    seedDetail([
      turn("u0", "user"),
      turn("a0", "assistant"),
      turn("u1", "user"),
      turn("a1", "assistant"),
    ])

    useConversationRuntimeStore
      .getState()
      .actions.appendOptimisticTurn(
        CONVERSATION_ID,
        turn("new", "user"),
        "token"
      )
    expect(baseline()).toBe(2)
  })

  it("captures a viewer boundary even when the echoed prompt is deduped", () => {
    seedDetail(
      [
        turn("u0", "user"),
        turn("a0", "assistant"),
        turn("prompt", "user"),
        turn("partial", "assistant"),
      ],
      "prompt"
    )

    useConversationRuntimeStore
      .getState()
      .actions.appendViewerUserTurn(CONVERSATION_ID, turn("prompt", "user"))
    expect(baseline()).toBe(1)
  })

  it("ignores a stale in-flight marker for a distinct owner prompt", () => {
    seedDetail(
      [
        turn("u0", "user"),
        turn("a0", "assistant"),
        turn("old-prompt", "user"),
        turn("a1", "assistant"),
      ],
      "old-prompt"
    )

    useConversationRuntimeStore
      .getState()
      .actions.appendOptimisticTurn(
        CONVERSATION_ID,
        turn("brand-new", "user"),
        "token"
      )
    expect(baseline()).toBe(2)
  })
})
