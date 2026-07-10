"use client"

import { useCallback, useEffect, useState } from "react"
import {
  EllipsisVertical,
  Menu,
  PanelRight,
  Search,
  Settings,
  SquareTerminal,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useIsActiveChatMode } from "@/hooks/use-is-active-chat-mode"
import { isDesktop, openFileDialog } from "@/lib/platform"
import { openSettingsWindow } from "@/lib/api"
import { getActiveRemoteConnectionId } from "@/lib/transport"
import { Button } from "@/components/ui/button"
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useTerminalContext } from "@/contexts/terminal-context"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useSearchDialog } from "@/contexts/search-dialog-context"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import {
  formatShortcutLabel,
  matchShortcutEvent,
} from "@/lib/keyboard-shortcuts"
import { AppTitleBar } from "./app-title-bar"
import { BranchDropdown } from "./branch-dropdown"
import { NewFolderDropdown } from "./new-folder-dropdown"
import { SearchCommandDialog } from "@/components/conversations/search-command-dialog"
import { DirectoryBrowserDialog } from "@/components/shared/directory-browser-dialog"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

export function FolderTitleBar() {
  const tTitleBar = useTranslations("Folder.folderTitleBar")
  const openFolder = useAppWorkspaceStore((s) => s.openFolder)
  const { activeFolder } = useActiveFolder()
  const isChatMode = useIsActiveChatMode()
  const { toggle } = useSidebarContext()
  const { isOpen: auxPanelOpen, toggle: toggleAuxPanel } = useAuxPanelContext()
  const { toggle: toggleTerminal } = useTerminalContext()
  const { openNewConversationTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const isMac = useIsMac()
  const { shortcuts } = useShortcutSettings()
  // Search open-state is shared (see search-dialog-context): this always-mounted
  // title bar owns the trigger, dialog, and the ⌘K shortcut.
  const { open: searchOpen, setOpen: setSearchOpen } = useSearchDialog()
  const [browserOpen, setBrowserOpen] = useState(false)
  const searchLabel = tTitleBar("withShortcut", {
    label: tTitleBar("search"),
    shortcut: formatShortcutLabel(shortcuts.toggle_search, isMac),
  })

  const handleOpenFolder = useCallback(async () => {
    // See NewFolderDropdown / SidebarConversationList for the same logic:
    // the native Tauri dialog browses the LOCAL filesystem, so when the
    // user is bound to a remote workspace we must fall through to the
    // in-app DirectoryBrowserDialog (which browses the remote host via
    // the proxied `list_directory_entries`).
    if (isDesktop() && getActiveRemoteConnectionId() === null) {
      try {
        const result = await openFileDialog({
          directory: true,
          multiple: false,
        })
        if (!result) return
        const selected = Array.isArray(result) ? result[0] : result
        await openFolder(selected)
      } catch (err) {
        console.error("[FolderTitleBar] failed to open folder:", err)
      }
    } else {
      setBrowserOpen(true)
    }
  }, [openFolder])

  const handleOpenSettings = useCallback(() => {
    openSettingsWindow("appearance").catch((err) => {
      console.error("[FolderTitleBar] failed to open settings:", err)
    })
  }, [])

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (matchShortcutEvent(e, shortcuts.toggle_search)) {
        e.preventDefault()
        setSearchOpen((prev) => !prev)
        return
      }
      if (matchShortcutEvent(e, shortcuts.toggle_sidebar)) {
        e.preventDefault()
        toggle()
        return
      }
      if (matchShortcutEvent(e, shortcuts.toggle_aux_panel)) {
        // Chat mode hides the aux panel + its toggle; the shortcut must not
        // re-open it either.
        if (isChatMode) return
        e.preventDefault()
        toggleAuxPanel()
        return
      }
      if (matchShortcutEvent(e, shortcuts.new_conversation)) {
        if (!activeFolder) return
        e.preventDefault()
        // Return to the conversation workspace if a route (e.g. Automations)
        // was covering the content region, else the new tab opens unseen.
        openConversations()
        openNewConversationTab(activeFolder.id, activeFolder.path)
        return
      }
      if (matchShortcutEvent(e, shortcuts.open_folder)) {
        e.preventDefault()
        void handleOpenFolder()
        return
      }
      if (matchShortcutEvent(e, shortcuts.open_settings)) {
        e.preventDefault()
        handleOpenSettings()
      }
    }
    document.addEventListener("keydown", handleKeyDown)
    return () => document.removeEventListener("keydown", handleKeyDown)
  }, [
    activeFolder,
    handleOpenFolder,
    handleOpenSettings,
    openConversations,
    openNewConversationTab,
    setSearchOpen,
    shortcuts,
    toggle,
    toggleAuxPanel,
    isChatMode,
  ])

  const isMobile = useIsMobile()
  return (
    <>
      <AppTitleBar
        left={
          isMobile ? (
            <div className="flex min-w-0 items-center gap-2">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8 shrink-0"
                onClick={toggle}
              >
                <Menu className="h-4 w-4" />
              </Button>
              <NewFolderDropdown />
              <BranchDropdown />
            </div>
          ) : (
            <div className="flex h-8 flex-1 items-center gap-6">
              <BranchDropdown />
              <div data-tauri-drag-region className="h-8 flex-1" />
            </div>
          )
        }
        right={
          isMobile ? (
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={() => setSearchOpen(true)}
                title={searchLabel}
                aria-label={searchLabel}
              >
                <Search className="h-4 w-4" />
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="icon" className="h-8 w-8">
                    <EllipsisVertical className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {/* Folderless chat conversations hide the aux panel entirely. */}
                  {!isChatMode && (
                    <DropdownMenuItem
                      onClick={toggleAuxPanel}
                      disabled={!activeFolder}
                    >
                      <PanelRight className="h-3.5 w-3.5" />
                      {tTitleBar("toggleAuxPanel")}
                    </DropdownMenuItem>
                  )}
                  <DropdownMenuItem
                    onClick={() => toggleTerminal()}
                    disabled={!activeFolder}
                  >
                    <SquareTerminal className="h-3.5 w-3.5" />
                    {tTitleBar("toggleTerminal")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={handleOpenSettings}>
                    <Settings className="h-3.5 w-3.5" />
                    {tTitleBar("openSettings")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              {/* Folderless chat conversations hide the aux panel entirely. */}
              {!isChatMode && (
                <Button
                  variant="ghost"
                  size="icon"
                  className={`h-6 w-6 hover:text-foreground/80 ${auxPanelOpen ? "bg-accent" : ""}`}
                  onClick={toggleAuxPanel}
                  disabled={!activeFolder}
                  title={tTitleBar("withShortcut", {
                    label: tTitleBar("toggleAuxPanel"),
                    shortcut: formatShortcutLabel(
                      shortcuts.toggle_aux_panel,
                      isMac
                    ),
                  })}
                >
                  <PanelRight className="h-3.5 w-3.5" />
                </Button>
              )}
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6 hover:text-foreground/80"
                onClick={() => setSearchOpen(true)}
                title={searchLabel}
                aria-label={searchLabel}
              >
                <Search className="h-3.5 w-3.5" />
              </Button>
            </div>
          )
        }
      />
      <SearchCommandDialog open={searchOpen} onOpenChange={setSearchOpen} />
      <DirectoryBrowserDialog
        open={browserOpen}
        onOpenChange={setBrowserOpen}
        onSelect={(path) => {
          openFolder(path).catch((err) => {
            console.error("[FolderTitleBar] failed to open folder:", err)
          })
        }}
      />
    </>
  )
}
