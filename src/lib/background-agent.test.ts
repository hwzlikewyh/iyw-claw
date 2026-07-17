import { describe, expect, it } from "vitest"

import {
  BACKGROUND_TASK_MARKER,
  isAsyncLaunchAckText,
  parseBackgroundTaskMarker,
} from "@/lib/background-agent"

describe("background task marker", () => {
  it("parses settled and pending lifecycle markers", () => {
    expect(
      parseBackgroundTaskMarker(
        `${BACKGROUND_TASK_MARKER}{"task_id":"agent-1","status":"completed","summary":"Done","result":"Build OK"}`
      )
    ).toEqual({
      taskId: "agent-1",
      status: "completed",
      summary: "Done",
      result: "Build OK",
    })
    expect(
      parseBackgroundTaskMarker(
        `${BACKGROUND_TASK_MARKER}{"task_id":"agent-2","status":null,"summary":null,"result":null}`
      )
    ).toEqual({
      taskId: "agent-2",
      status: null,
      summary: null,
      result: null,
    })
  })

  it("rejects malformed markers and recognizes only live async launch acks", () => {
    expect(parseBackgroundTaskMarker("plain output")).toBeNull()
    expect(
      parseBackgroundTaskMarker(`${BACKGROUND_TASK_MARKER}{bad`)
    ).toBeNull()
    expect(
      isAsyncLaunchAckText("Async agent launched successfully. agentId: a1")
    ).toBe(true)
    expect(isAsyncLaunchAckText("Agent completed")).toBe(false)
  })
})
