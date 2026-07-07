import { fireEvent, render, waitFor } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { Sidebar } from "./sidebar"
import enMessages from "@/i18n/messages/en.json"

// Stable spies + mutable active-folder, referenced from the hoisted mock
// factories below (vi.mock is hoisted above imports).
const spies = vi.hoisted(() => ({
  openNewConversationTab: vi.fn(),
  openChatModeTab: vi.fn(),
  setRoute: vi.fn(),
  openConversations: vi.fn(),
}))
const apiMocks = vi.hoisted(() => ({
  getWebServerStatus: vi.fn(),
  getWebServiceConfig: vi.fn(),
  iywAccountGetProfile: vi.fn(),
  iywAccountGetWechatQrcode: vi.fn(),
  iywAccountPollWechatLogin: vi.fn(),
  iywAccountLoginWithPassword: vi.fn(),
  iywAccountLogout: vi.fn(),
}))
const mockState = vi.hoisted(() => ({
  activeFolder: { id: 7, path: "/x" } as { id: number; path: string } | null,
}))

// The conversation list is irrelevant here — stub it so the test exercises only
// the sidebar's header + fixed action region.
vi.mock("@/components/conversations/sidebar-conversation-list", () => ({
  SidebarConversationList: () => null,
}))
vi.mock("@/contexts/sidebar-context", () => ({
  useSidebarContext: () => ({ isOpen: true, toggle: vi.fn() }),
}))
vi.mock("@/contexts/active-folder-context", () => ({
  useActiveFolder: () => ({ activeFolder: mockState.activeFolder }),
}))
vi.mock("@/contexts/tab-context", () => ({
  useTabActions: () => ({
    openNewConversationTab: spies.openNewConversationTab,
    openChatModeTab: spies.openChatModeTab,
  }),
}))
vi.mock("@/contexts/automations-view-context", () => ({
  useAutomationsView: () => ({
    automations: [],
    unseenFailures: 0,
    refetch: async () => {},
  }),
}))
vi.mock("@/contexts/workbench-route-context", () => ({
  useWorkbenchRoute: () => ({
    routeId: "conversations",
    isConversations: true,
    setRoute: spies.setRoute,
    openConversations: spies.openConversations,
  }),
}))
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn() }),
}))
vi.mock("@/hooks/use-is-mac", () => ({ useIsMac: () => false }))
vi.mock("@/hooks/use-shortcut-settings", () => ({
  useShortcutSettings: () => ({
    shortcuts: {
      toggle_search: "mod+k",
      toggle_sidebar: "mod+b",
      new_conversation: "mod+t",
      open_settings: "mod+,",
    },
  }),
}))
vi.mock("@/hooks/use-mobile", () => ({ useIsMobile: () => false }))
vi.mock("@/lib/api", () => ({
  getWebServerStatus: apiMocks.getWebServerStatus,
  getWebServiceConfig: apiMocks.getWebServiceConfig,
  iywAccountGetProfile: apiMocks.iywAccountGetProfile,
  iywAccountGetWechatQrcode: apiMocks.iywAccountGetWechatQrcode,
  iywAccountPollWechatLogin: apiMocks.iywAccountPollWechatLogin,
  iywAccountLoginWithPassword: apiMocks.iywAccountLoginWithPassword,
  iywAccountLogout: apiMocks.iywAccountLogout,
}))

function renderSidebar() {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      <Sidebar />
    </NextIntlClientProvider>
  )
}

describe("Sidebar — fixed action region", () => {
  beforeEach(() => {
    spies.openNewConversationTab.mockClear()
    spies.openChatModeTab.mockClear()
    spies.setRoute.mockClear()
    spies.openConversations.mockClear()
    mockState.activeFolder = { id: 7, path: "/x" }
    apiMocks.getWebServerStatus.mockReset()
    apiMocks.getWebServiceConfig.mockReset()
    apiMocks.iywAccountGetProfile.mockReset()
    apiMocks.iywAccountGetWechatQrcode.mockReset()
    apiMocks.iywAccountPollWechatLogin.mockReset()
    apiMocks.iywAccountLoginWithPassword.mockReset()
    apiMocks.iywAccountLogout.mockReset()
    apiMocks.getWebServerStatus.mockResolvedValue({
      port: 3080,
      token: "test-token",
      addresses: ["http://localhost:3080"],
    })
    apiMocks.getWebServiceConfig.mockResolvedValue({
      token: "test-token",
      port: 3080,
      autoStart: true,
    })
    apiMocks.iywAccountGetProfile.mockResolvedValue({
      logged_in: false,
      user_id: null,
      name: null,
      nick_name: null,
      phone: null,
      avatar_url: null,
      org_name: null,
      org_logo_url: null,
      balance_points: null,
      balance_expiry_time: null,
    })
    apiMocks.iywAccountGetWechatQrcode.mockResolvedValue({
      qrcode_url: "https://example.com/qrcode.png",
      qr_token: "qr-token",
    })
    apiMocks.iywAccountPollWechatLogin.mockResolvedValue({
      status: "pending",
      profile: null,
    })
    apiMocks.iywAccountLoginWithPassword.mockResolvedValue({
      logged_in: true,
      user_id: "u1",
      name: "Test User",
      nick_name: null,
      phone: "123",
      avatar_url: null,
      org_name: null,
      org_logo_url: null,
      balance_points: 4228,
      balance_expiry_time: null,
    })
    apiMocks.iywAccountLogout.mockResolvedValue(undefined)
  })

  it("Automations navigates to the automations route", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("Not signed in")

    fireEvent.click(getByText("Automations"))
    expect(spies.setRoute).toHaveBeenCalledWith("automations")
  })

  it("New chat returns to the conversation workspace", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("Not signed in")

    fireEvent.click(getByText("New chat"))
    expect(spies.openConversations).toHaveBeenCalled()
  })

  it("New chat opens a conversation tab in the active folder", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("Not signed in")

    fireEvent.click(getByText("New chat"))
    expect(spies.openNewConversationTab).toHaveBeenCalledWith(7, "/x")
  })

  it("renders the New chat shortcut hint", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("Not signed in")

    // isMac=false → "mod" formats as "Ctrl". The badges are opacity-0 until the
    // row is hovered/focused but stay in the DOM, so getByText resolves them.
    expect(getByText("Ctrl+T")).toBeTruthy()
  })

  it("opens the account dialog without the settings panel", async () => {
    const { findByText, queryByText } = renderSidebar()

    fireEvent.click(await findByText("Not signed in"))
    expect(await findByText("iyw Account")).toBeTruthy()
    expect(await findByText("WeChat")).toBeTruthy()
    expect(await findByText("Password")).toBeTruthy()
    expect(queryByText("Web Service")).toBeNull()
  })

  it("signs in with account password from the dialog", async () => {
    const { findByLabelText, findByText, getByText, queryByText } =
      renderSidebar()

    fireEvent.click(await findByText("Not signed in"))
    fireEvent.click(getByText("Password"))

    fireEvent.change(await findByLabelText("Account"), {
      target: { value: "alice" },
    })
    fireEvent.change(await findByLabelText("Password"), {
      target: { value: "secret" },
    })
    fireEvent.click(getByText("Sign in"))

    await waitFor(() => {
      expect(apiMocks.iywAccountLoginWithPassword).toHaveBeenCalledWith({
        username: "alice",
        password: "secret",
      })
    })
    await waitFor(() => {
      expect(queryByText("Sign in with account")).toBeNull()
    })
    expect(await findByText("Test User")).toBeTruthy()
    expect(await findByText("4228")).toBeTruthy()
  })

  it("falls back to chat mode (never disabled) when no folder is active", async () => {
    mockState.activeFolder = null
    const { findByText, getByText } = renderSidebar()
    await findByText("Not signed in")

    const btn = getByText("New chat").closest("button") as HTMLButtonElement
    // Defense-in-depth: the button stays clickable so a workspace that recovered
    // to no active folder is never a dead end — it opens folderless chat mode.
    expect(btn.disabled).toBe(false)
    fireEvent.click(btn)
    expect(spies.openChatModeTab).toHaveBeenCalled()
    expect(spies.openNewConversationTab).not.toHaveBeenCalled()
  })
})
