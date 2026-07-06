import { fireEvent, render, screen } from "@testing-library/react"
import { beforeAll, describe, expect, it, vi } from "vitest"

import { SlashCommandMenu } from "./slash-command-menu"
import type { AvailableCommandInfo } from "@/lib/types"

// jsdom does not implement `Element.prototype.scrollIntoView`. The component
// calls it in a useEffect every time `selectedIndex` changes; if missing it
// throws synchronously and fails the render. Polyfill once for the suite.
beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn()
})

const sampleCommands: AvailableCommandInfo[] = [
  { name: "clear", description: "Clear the conversation" },
  { name: "help", description: "Show available commands" },
  { name: "model", description: "Switch model", input_hint: "model name" },
]

describe("SlashCommandMenu", () => {
  it("renders nothing when commands list is empty", () => {
    const { container } = render(
      <SlashCommandMenu commands={[]} selectedIndex={0} onSelect={() => {}} />
    )
    expect(container.firstChild).toBeNull()
  })

  it("renders every command with a /name prefix and description", () => {
    render(
      <SlashCommandMenu
        commands={sampleCommands}
        selectedIndex={0}
        onSelect={() => {}}
      />
    )
    expect(screen.getByText("/clear")).toBeInTheDocument()
    expect(screen.getByText("/help")).toBeInTheDocument()
    expect(screen.getByText("/model")).toBeInTheDocument()
    expect(screen.getByText("Clear the conversation")).toBeInTheDocument()
    expect(screen.getByText("Show available commands")).toBeInTheDocument()
  })

  it("applies the accent style only to the selected row", () => {
    render(
      <SlashCommandMenu
        commands={sampleCommands}
        selectedIndex={1}
        onSelect={() => {}}
      />
    )
    const buttons = screen.getAllByRole("button")
    expect(buttons[0].className).not.toContain("bg-accent")
    expect(buttons[1].className).toContain("bg-accent")
    expect(buttons[2].className).not.toContain("bg-accent")
  })

  it("invokes onSelect with the clicked command on mousedown", () => {
    const onSelect = vi.fn()
    render(
      <SlashCommandMenu
        commands={sampleCommands}
        selectedIndex={0}
        onSelect={onSelect}
      />
    )
    fireEvent.mouseDown(screen.getByText("/help"))
    expect(onSelect).toHaveBeenCalledTimes(1)
    expect(onSelect).toHaveBeenCalledWith(sampleCommands[1])
  })

  it("prevents default on mousedown so the chat input keeps focus", () => {
    // The component uses onMouseDown + preventDefault so clicking a menu
    // entry doesn't blur the input behind it. Regression guard for that.
    render(
      <SlashCommandMenu
        commands={sampleCommands}
        selectedIndex={0}
        onSelect={() => {}}
      />
    )
    const target = screen.getByText("/clear")
    const event = new MouseEvent("mousedown", {
      bubbles: true,
      cancelable: true,
    })
    const defaultPrevented = !target.dispatchEvent(event)
    expect(defaultPrevented).toBe(true)
  })
})
