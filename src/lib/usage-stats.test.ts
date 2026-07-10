import { describe, expect, it } from "vitest"
import {
  aggregateUsageStats,
  cacheHitRate,
  usageTotal,
  type UsageBreakdown,
} from "./usage-stats"
import type { ConversationDetail } from "./types"

function detail(params: {
  id: string
  startedAt: string
  model?: string | null
  turnModel?: string | null
  input: number
  output: number
  cacheRead?: number
  cacheWrite?: number
}): ConversationDetail {
  return {
    summary: {
      id: params.id,
      agent_type: "codex",
      folder_path: null,
      folder_name: null,
      title: null,
      started_at: params.startedAt,
      ended_at: null,
      message_count: 1,
      model: params.model ?? null,
      git_branch: null,
    },
    turns: params.turnModel
      ? [
          {
            id: `${params.id}-turn`,
            role: "assistant",
            blocks: [],
            timestamp: params.startedAt,
            model: params.turnModel,
          },
        ]
      : [],
    session_stats: {
      total_usage: {
        input_tokens: params.input,
        output_tokens: params.output,
        cache_read_input_tokens: params.cacheRead ?? 0,
        cache_creation_input_tokens: params.cacheWrite ?? 0,
      },
      total_duration_ms: 0,
    },
  }
}

describe("usage stats aggregation", () => {
  it("aggregates total, model and daily usage rows", () => {
    const stats = aggregateUsageStats(
      [
        detail({
          id: "a",
          startedAt: "2026-07-08T10:00:00Z",
          model: "gpt-5.5",
          input: 100,
          output: 20,
          cacheRead: 50,
          cacheWrite: 10,
        }),
        detail({
          id: "b",
          startedAt: "2026-07-09T10:00:00Z",
          turnModel: "deepseek-v4-flash",
          input: 200,
          output: 30,
          cacheRead: 70,
        }),
      ],
      { dayCount: 3, now: new Date("2026-07-09T00:00:00Z") }
    )

    expect(stats.sessionCount).toBe(2)
    expect(stats.totalTokens).toBe(480)
    expect(stats.total).toEqual({
      input: 300,
      output: 50,
      cacheRead: 120,
      cacheWrite: 10,
    })
    expect(stats.averageDailySessions).toBe(1)
    expect(stats.modelRows.map((row) => row.model)).toEqual([
      "deepseek-v4-flash",
      "gpt-5.5",
    ])
    expect(stats.dailyRows.map((row) => row.date)).toEqual([
      "2026-07-08",
      "2026-07-09",
    ])
    expect(stats.dailyRows[1]).toMatchObject({
      sessions: 1,
      input: 200,
      output: 30,
      cacheRead: 70,
      total: 300,
    })
  })

  it("uses zero-filled recent days when there is no usage", () => {
    const stats = aggregateUsageStats([], {
      dayCount: 2,
      now: new Date("2026-07-09T00:00:00Z"),
    })

    expect(stats.sessionCount).toBe(0)
    expect(stats.totalTokens).toBe(0)
    expect(stats.dailyRows).toEqual([
      {
        date: "2026-07-08",
        sessions: 0,
        total: 0,
        cacheHitRate: 0,
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
      },
      {
        date: "2026-07-09",
        sessions: 0,
        total: 0,
        cacheHitRate: 0,
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
      },
    ])
  })

  it("computes token totals and cache hit rate", () => {
    const usage: UsageBreakdown = {
      input: 100,
      output: 20,
      cacheRead: 80,
      cacheWrite: 20,
    }

    expect(usageTotal(usage)).toBe(220)
    expect(cacheHitRate(usage)).toBe(0.4)
  })
})
