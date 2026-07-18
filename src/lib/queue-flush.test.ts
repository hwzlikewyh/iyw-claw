import { describe, expect, it } from "vitest"

import {
  shouldBlockUnboundSend,
  shouldQueueBeforeConnection,
} from "@/lib/queue-flush"

describe("offline-first chat queue", () => {
  it("queues a direct submit while the agent is still connecting", () => {
    expect(shouldQueueBeforeConnection(false, false)).toBe(true)
  })

  it("lets the queue flush send once the connection is ready", () => {
    expect(shouldQueueBeforeConnection(true, true)).toBe(false)
  })

  it("does not requeue a flush solely because the connection is stale", () => {
    expect(shouldQueueBeforeConnection(false, true)).toBe(false)
  })

  it("allows a first message while the Agent list is still loading", () => {
    expect(shouldBlockUnboundSend(false, false, 0)).toBe(false)
  })

  it("blocks only after loading proves that no Agent is usable", () => {
    expect(shouldBlockUnboundSend(false, true, 0)).toBe(true)
    expect(shouldBlockUnboundSend(false, true, 1)).toBe(false)
    expect(shouldBlockUnboundSend(true, true, 0)).toBe(false)
  })
})
