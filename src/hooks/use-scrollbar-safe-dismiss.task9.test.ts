import { act, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { useScrollbarSafeDismiss } from "@/hooks/use-scrollbar-safe-dismiss"

const attached: HTMLElement[] = []

afterEach(() => {
  attached.splice(0).forEach((element) => element.remove())
  vi.restoreAllMocks()
})

function mountGuard() {
  const { result } = renderHook(() => useScrollbarSafeDismiss())
  const content = document.createElement("div")
  document.body.appendChild(content)
  attached.push(content)
  result.current.contentRef.current = content
  return { result, content }
}

function focusOutside(
  handler: (event: CustomEvent<{ originalEvent: FocusEvent }>) => void,
  target: EventTarget | null
) {
  const preventDefault = vi.fn()
  handler({
    detail: { originalEvent: { target } as FocusEvent },
    preventDefault,
  } as unknown as CustomEvent<{ originalEvent: FocusEvent }>)
  return preventDefault
}

describe("useScrollbarSafeDismiss", () => {
  it("keeps the layer open when an inside scrollbar grab bounces focus outside", () => {
    const { result, content } = mountGuard()
    const handle = document.createElement("div")
    content.appendChild(handle)
    const outside = document.createElement("div")
    document.body.appendChild(outside)
    attached.push(outside)

    act(() => handle.dispatchEvent(new Event("pointerdown", { bubbles: true })))

    expect(
      focusOutside(result.current.onFocusOutside, outside)
    ).toHaveBeenCalledTimes(1)
  })

  it("allows focus dismissal when the pointer interaction began outside", () => {
    const { result } = mountGuard()
    const outside = document.createElement("input")
    document.body.appendChild(outside)
    attached.push(outside)

    act(() =>
      outside.dispatchEvent(new Event("pointerdown", { bubbles: true }))
    )

    expect(
      focusOutside(result.current.onFocusOutside, outside)
    ).not.toHaveBeenCalled()
  })
})
