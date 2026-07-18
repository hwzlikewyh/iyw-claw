import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { SidebarToggleButton } from "./sidebar-nav-button"

describe("SidebarToggleButton", () => {
  it("keeps the same focused control when the sidebar state changes", () => {
    const onClick = vi.fn()
    const { getByRole, rerender } = render(
      <SidebarToggleButton isOpen label="Hide sidebar" onClick={onClick} />
    )
    const before = getByRole("button", { name: "Hide sidebar" })
    before.focus()

    rerender(
      <SidebarToggleButton
        isOpen={false}
        label="Show sidebar"
        onClick={onClick}
      />
    )

    const after = getByRole("button", { name: "Show sidebar" })
    expect(after).toBe(before)
    expect(document.activeElement).toBe(after)

    fireEvent.click(after)
    expect(onClick).toHaveBeenCalledOnce()
  })
})
