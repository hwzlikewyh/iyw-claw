import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { IywAccountProvider, useIywAccount } from "./iyw-account-context"

const { getProfile, loginWithPassword, logout } = vi.hoisted(() => ({
  getProfile: vi.fn(),
  loginWithPassword: vi.fn(),
  logout: vi.fn(),
}))

vi.mock("@/lib/api", () => ({
  iywAccountGetProfile: getProfile,
  iywAccountLoginWithPassword: loginWithPassword,
  iywAccountLogout: logout,
}))

const profile = {
  logged_in: true,
  user_id: "user-1",
  name: "Test User",
  nick_name: null,
  phone: null,
  avatar_url: null,
  org_name: null,
  org_logo_url: null,
  balance_points: 42,
  balance_expiry_time: null,
}

const networkError = {
  code: "network_error",
  message: "balance unavailable",
}

function Probe() {
  const account = useIywAccount()
  return (
    <>
      <span data-testid="status">{account.status}</span>
      <span data-testid="points">
        {account.profile?.balance_points ?? "none"}
      </span>
      <button onClick={() => void account.refreshProfile()}>refresh</button>
      <button onClick={() => void account.logout()}>logout</button>
    </>
  )
}

describe("IywAccountProvider", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getProfile.mockResolvedValueOnce(profile)
    logout.mockResolvedValue(undefined)
  })

  it("retains the last profile on transient refresh failure but clears it on logout", async () => {
    render(
      <IywAccountProvider>
        <Probe />
      </IywAccountProvider>
    )

    await waitFor(() =>
      expect(screen.getByTestId("status").textContent).toBe("authenticated")
    )
    expect(screen.getByTestId("points").textContent).toBe("42")

    getProfile.mockRejectedValueOnce(networkError)
    fireEvent.click(screen.getByText("refresh"))
    await waitFor(() =>
      expect(screen.getByTestId("status").textContent).toBe("authenticated")
    )
    expect(screen.getByTestId("points").textContent).toBe("42")

    fireEvent.click(screen.getByText("logout"))
    await waitFor(() =>
      expect(screen.getByTestId("status").textContent).toBe("login_required")
    )
    expect(screen.getByTestId("points").textContent).toBe("none")

    getProfile.mockRejectedValueOnce(networkError)
    fireEvent.click(screen.getByText("refresh"))
    await waitFor(() =>
      expect(screen.getByTestId("status").textContent).toBe("error")
    )
    expect(screen.getByTestId("points").textContent).toBe("none")
  })
})
