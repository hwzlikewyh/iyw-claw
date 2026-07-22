import "@testing-library/jest-dom/vitest"

import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { MessageTimestamp } from "./message-timestamp"

vi.mock("next-intl", () => ({
  useLocale: () => "en-US",
}))

describe("MessageTimestamp", () => {
  it("renders the localized message time with the full date as a tooltip", () => {
    const date = new Date(2026, 6, 22, 14, 5, 6)
    const timestamp = date.toISOString()

    render(<MessageTimestamp timestamp={timestamp} />)

    const time = screen.getByText(
      new Intl.DateTimeFormat("en-US", {
        hour: "2-digit",
        minute: "2-digit",
      }).format(date)
    )
    expect(time).toHaveAttribute("dateTime", timestamp)
    expect(time).toHaveAttribute(
      "title",
      new Intl.DateTimeFormat("en-US", {
        dateStyle: "medium",
        timeStyle: "medium",
      }).format(date)
    )
  })

  it("does not render an invalid timestamp", () => {
    const { container } = render(<MessageTimestamp timestamp="invalid" />)

    expect(container).toBeEmptyDOMElement()
  })
})
