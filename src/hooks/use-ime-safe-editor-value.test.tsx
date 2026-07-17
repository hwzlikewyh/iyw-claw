import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
  useImeSafeEditorValue,
  type ImeCompositionEditor,
} from "@/hooks/use-ime-safe-editor-value"

type EventName = "start" | "end" | "model" | "dispose"

function fakeEditor() {
  const listeners = new Map<EventName, () => void>()
  const register = (name: EventName, listener: () => void) => {
    listeners.set(name, listener)
    return { dispose: () => listeners.delete(name) }
  }
  return {
    editor: {
      onDidCompositionStart: (listener: () => void) =>
        register("start", listener),
      onDidCompositionEnd: (listener: () => void) => register("end", listener),
      onDidChangeModel: (listener: () => void) => register("model", listener),
      onDidDispose: (listener: () => void) => register("dispose", listener),
    } satisfies ImeCompositionEditor,
    emit: (name: EventName) => listeners.get(name)?.(),
  }
}

describe("useImeSafeEditorValue", () => {
  let frame: FrameRequestCallback | null

  beforeEach(() => {
    frame = null
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frame = callback
      return 1
    })
    vi.stubGlobal("cancelAnimationFrame", vi.fn())
  })

  it("leaves Monaco uncontrolled during composition and resumes next frame", () => {
    const changes = vi.fn()
    const { editor, emit } = fakeEditor()
    const { result, rerender } = renderHook(
      ({ value }) => useImeSafeEditorValue(value, "tab-1", changes),
      { initialProps: { value: "before" } }
    )

    act(() => result.current.bindEditor(editor))
    act(() => emit("start"))
    rerender({ value: "intermediate" })
    expect(result.current.value).toBeUndefined()
    expect(result.current.isComposing).toBe(true)

    act(() => emit("end"))
    expect(result.current.value).toBeUndefined()
    act(() => frame?.(0))

    expect(result.current.value).toBe("intermediate")
    expect(result.current.isComposing).toBe(false)
    expect(changes.mock.calls).toEqual([
      [true, "tab-1"],
      [false, "tab-1"],
    ])
  })
})
