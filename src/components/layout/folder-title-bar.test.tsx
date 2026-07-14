import { render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { FolderTitleBar } from "./folder-title-bar"

const mocks = vi.hoisted(() => ({
  openFolder: vi.fn(),
  openSettingsWindow: vi.fn(),
  setSearchOpen: vi.fn(),
  toggle: vi.fn(),
}))

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))
vi.mock("@/stores/app-workspace-store", () => ({
  useAppWorkspaceStore: (selector: (state: unknown) => unknown) =>
    selector({ openFolder: mocks.openFolder }),
}))
vi.mock("@/contexts/active-folder-context", () => ({
  useActiveFolder: () => ({ activeFolder: null }),
}))
vi.mock("@/hooks/use-is-active-chat-mode", () => ({
  useIsActiveChatMode: () => false,
}))
vi.mock("@/lib/platform", () => ({
  isDesktop: () => false,
  openFileDialog: vi.fn(),
}))
vi.mock("@/lib/api", () => ({
  openSettingsWindow: mocks.openSettingsWindow,
}))
vi.mock("@/lib/transport", () => ({
  getActiveRemoteConnectionId: () => null,
}))
vi.mock("@/contexts/sidebar-context", () => ({
  useSidebarContext: () => ({ toggle: mocks.toggle }),
}))
vi.mock("@/contexts/aux-panel-context", () => ({
  useAuxPanelContext: () => ({ isOpen: false, toggle: mocks.toggle }),
}))
vi.mock("@/contexts/terminal-context", () => ({
  useTerminalContext: () => ({ toggle: mocks.toggle }),
}))
vi.mock("@/contexts/tab-context", () => ({
  useTabActions: () => ({ openNewConversationTab: vi.fn() }),
}))
vi.mock("@/contexts/workbench-route-context", () => ({
  useWorkbenchRoute: () => ({ openConversations: vi.fn() }),
}))
vi.mock("@/contexts/search-dialog-context", () => ({
  useSearchDialog: () => ({ open: false, setOpen: mocks.setSearchOpen }),
}))
vi.mock("@/hooks/use-is-mac", () => ({ useIsMac: () => false }))
vi.mock("@/hooks/use-shortcut-settings", () => ({
  useShortcutSettings: () => ({
    shortcuts: {
      toggle_search: "mod+k",
      toggle_sidebar: "mod+b",
      toggle_aux_panel: "mod+shift+b",
      new_conversation: "mod+t",
      open_folder: "mod+o",
      open_settings: "mod+,",
    },
  }),
}))
vi.mock("@/hooks/use-mobile", () => ({ useIsMobile: () => false }))
vi.mock("./app-title-bar", () => ({
  AppTitleBar: ({ left }: { left?: React.ReactNode }) => (
    <header data-testid="app-title-bar">{left}</header>
  ),
}))
vi.mock("./branch-dropdown", () => ({ BranchDropdown: () => null }))
vi.mock("./new-folder-dropdown", () => ({ NewFolderDropdown: () => null }))
vi.mock("@/components/conversations/search-command-dialog", () => ({
  SearchCommandDialog: () => null,
}))
vi.mock("@/components/shared/directory-browser-dialog", () => ({
  DirectoryBrowserDialog: () => null,
}))

describe("FolderTitleBar", () => {
  beforeEach(() => vi.clearAllMocks())

  it("shows the branded version at the far-left of the top application bar", () => {
    render(<FolderTitleBar />)

    const titleBar = screen.getByTestId("app-title-bar")
    expect(titleBar.firstElementChild?.textContent).toContain("原助手 v0.0.2")
  })
})
