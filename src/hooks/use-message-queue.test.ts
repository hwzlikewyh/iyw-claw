import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import type { PromptDraft } from "@/lib/types"
import { useMessageQueue } from "./use-message-queue"

const draft: PromptDraft = {
  blocks: [{ type: "text", text: "retry me" }],
  displayText: "retry me",
}

describe("useMessageQueue", () => {
  it("keeps a blocked item until an explicit update clears it", () => {
    const { result } = renderHook(() => useMessageQueue())

    act(() => {
      result.current.enqueue(draft, null, { blocked: true })
    })
    expect(result.current.queue[0]?.blocked).toBe(true)

    act(() => {
      result.current.updateItem(result.current.queue[0]!.id, draft)
    })
    expect(result.current.queue[0]?.blocked).toBe(false)
  })
})
