import { describe, expect, it } from "vitest"
import { loadUsageDetails, type UsageDetailLoader } from "./usage-detail-loader"
import type {
  AgentType,
  ConversationDetail,
  ConversationSummary,
} from "./types"

function summary(id: string): ConversationSummary {
  return {
    id,
    agent_type: "codex",
    folder_path: null,
    folder_name: null,
    title: null,
    started_at: "2026-07-09T00:00:00Z",
    ended_at: null,
    message_count: 1,
    model: null,
    git_branch: null,
  }
}

function detail(
  id: string,
  agentType: AgentType = "codex"
): ConversationDetail {
  return {
    summary: {
      ...summary(id),
      agent_type: agentType,
    },
    turns: [],
    session_stats: null,
  }
}

describe("usage detail loader", () => {
  it("limits concurrent detail loads", async () => {
    let running = 0
    let maxRunning = 0
    const loader: UsageDetailLoader = async (conversation) => {
      running += 1
      maxRunning = Math.max(maxRunning, running)
      await Promise.resolve()
      running -= 1
      return detail(conversation.id, conversation.agent_type)
    }

    const result = await loadUsageDetails(
      [summary("a"), summary("b"), summary("c"), summary("d"), summary("e")],
      {
        concurrency: 2,
        loadConversation: loader,
      }
    )

    expect(maxRunning).toBeLessThanOrEqual(2)
    expect(result.details.map((item) => item.summary.id)).toEqual([
      "a",
      "b",
      "c",
      "d",
      "e",
    ])
    expect(result.failedConversations).toBe(0)
  })

  it("counts failed conversations without aborting the batch", async () => {
    const loader: UsageDetailLoader = async (conversation) => {
      if (conversation.id === "b") throw new Error("broken")
      return detail(conversation.id, conversation.agent_type)
    }

    const result = await loadUsageDetails(
      [summary("a"), summary("b"), summary("c")],
      {
        concurrency: 2,
        loadConversation: loader,
      }
    )

    expect(result.details.map((item) => item.summary.id)).toEqual(["a", "c"])
    expect(result.failedConversations).toBe(1)
  })

  it("stops scheduling new batches when the request is stale", async () => {
    let current = true
    const loadedIds: string[] = []
    const loader: UsageDetailLoader = async (conversation) => {
      loadedIds.push(conversation.id)
      if (conversation.id === "b") current = false
      return detail(conversation.id, conversation.agent_type)
    }

    const result = await loadUsageDetails(
      [summary("a"), summary("b"), summary("c"), summary("d")],
      {
        concurrency: 2,
        isCurrent: () => current,
        loadConversation: loader,
      }
    )

    expect(loadedIds).toEqual(["a", "b"])
    expect(result.details).toEqual([])
    expect(result.failedConversations).toBe(0)
  })
})
