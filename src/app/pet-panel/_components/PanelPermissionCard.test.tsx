import { render, screen, fireEvent, cleanup } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import enMessages from "@/i18n/messages/en.json"
import zhMessages from "@/i18n/messages/zh-CN.json"

// Stub the backend command with a never-resolving promise so `busy` stays true
// after the first click (models an in-flight response).
vi.mock("@/lib/api", () => ({
  acpRespondPermission: vi.fn(() => new Promise<void>(() => {})),
}))

import { acpRespondPermission } from "@/lib/api"
import { PanelPermissionCard } from "./PanelPermissionCard"

const permission = {
  requestId: "r1",
  toolCall: { tool_name: "Bash", rawInput: { command: "ls -la" } },
  options: [
    { option_id: "allow", name: "Allow", kind: "allow_once" },
    { option_id: "reject", name: "Reject", kind: "reject_once" },
  ],
}

function renderPanel(locale: "en" | "zh-CN" = "en", messages = enMessages) {
  return render(
    <NextIntlClientProvider locale={locale} messages={messages}>
      <PanelPermissionCard connectionId="c1" permission={permission} />
    </NextIntlClientProvider>
  )
}

describe("PanelPermissionCard", () => {
  beforeEach(() => vi.clearAllMocks())
  afterEach(() => cleanup())

  it("forwards a single response with the right ids", () => {
    renderPanel()
    fireEvent.click(screen.getByRole("button", { name: "Allow once" }))
    expect(acpRespondPermission).toHaveBeenCalledTimes(1)
    expect(acpRespondPermission).toHaveBeenCalledWith("c1", "r1", "allow")
  })

  it("ignores rapid double-clicks while a response is in flight", () => {
    renderPanel()
    const allow = screen.getByRole("button", { name: "Allow once" })
    fireEvent.click(allow)
    fireEvent.click(allow)
    fireEvent.click(allow)
    expect(acpRespondPermission).toHaveBeenCalledTimes(1)
  })

  it("localizes permission options", () => {
    renderPanel("zh-CN", zhMessages)
    expect(
      screen.getByRole("button", { name: "仅允许一次" })
    ).toBeInTheDocument()
    expect(screen.getByRole("button", { name: "拒绝" })).toBeInTheDocument()
  })
})
