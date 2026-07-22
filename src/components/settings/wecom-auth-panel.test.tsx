import "@testing-library/jest-dom/vitest"

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { WecomAuthPanel } from "./wecom-auth-panel"

const { wecomGetAuthStatus, wecomStartAuth } = vi.hoisted(() => ({
  wecomGetAuthStatus: vi.fn(),
  wecomStartAuth: vi.fn(),
}))

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))

vi.mock("@/lib/api", () => ({
  wecomGetAuthStatus,
  wecomStartAuth,
}))

vi.mock("qrcode.react", () => ({
  QRCodeSVG: ({ value }: { value: string }) => (
    <div data-testid="wecom-auth-qrcode">{value}</div>
  ),
}))

describe("WecomAuthPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    wecomGetAuthStatus.mockResolvedValue({
      cli_installed: true,
      authorized: false,
    })
    wecomStartAuth.mockResolvedValue({
      auth_url: "https://work.weixin.qq.com/ai/qc/gen?source=test",
    })
  })

  it("shows the auth page link without re-encoding it as another QR code", async () => {
    const { unmount } = render(<WecomAuthPanel />)

    fireEvent.click(
      await screen.findByRole("button", { name: "wecomStartAuth" })
    )

    const link = await screen.findByRole("link", {
      name: "wecomOpenAuthPage",
    })
    expect(link).toHaveAttribute(
      "href",
      "https://work.weixin.qq.com/ai/qc/gen?source=test"
    )
    await waitFor(() =>
      expect(screen.queryByTestId("wecom-auth-qrcode")).not.toBeInTheDocument()
    )

    unmount()
  })
})
