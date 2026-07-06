import { fireEvent, render } from "@testing-library/react"
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
  })

  it("Automations navigates to the automations route", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("http://localhost:3080")

    fireEvent.click(getByText("Automations"))
    expect(spies.setRoute).toHaveBeenCalledWith("automations")
  })

  it("New chat returns to the conversation workspace", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("http://localhost:3080")

    fireEvent.click(getByText("New chat"))
    expect(spies.openConversations).toHaveBeenCalled()
  })

  it("New chat opens a conversation tab in the active folder", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("http://localhost:3080")

    fireEvent.click(getByText("New chat"))
    expect(spies.openNewConversationTab).toHaveBeenCalledWith(7, "/x")
  })

  it("renders the New chat shortcut hint", async () => {
    const { findByText, getByText } = renderSidebar()
    await findByText("http://localhost:3080")

    // isMac=false → "mod" formats as "Ctrl". The badges are opacity-0 until the
    // row is hovered/focused but stay in the DOM, so getByText resolves them.
    expect(getByText("Ctrl+T")).toBeTruthy()
  })

  it("renders settings and Web access information in the bottom region", async () => {
    const { findByText, getByText } = renderSidebar()

    expect(getByText("Settings")).toBeTruthy()
    expect(await findByText("Web Service")).toBeTruthy()
    expect(await findByText("http://localhost:3080")).toBeTruthy()
    expect(await findByText("********")).toBeTruthy()
  })

  it("falls back to chat mode (never disabled) when no folder is active", async () => {
    mockState.activeFolder = null
    const { findByText, getByText } = renderSidebar()
    await findByText("http://localhost:3080")

    const btn = getByText("New chat").closest("button") as HTMLButtonElement
    // Defense-in-depth: the button stays clickable so a workspace that recovered
    // to no active folder is never a dead end — it opens folderless chat mode.
    expect(btn.disabled).toBe(false)
    fireEvent.click(btn)
    expect(spies.openChatModeTab).toHaveBeenCalled()
    expect(spies.openNewConversationTab).not.toHaveBeenCalled()
  })
})
