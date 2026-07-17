"use client"

import { useCallback, useRef, useState } from "react"
import {
  ChevronsDownUp,
  ChevronsUpDown,
  Crosshair,
  PackageCheck,
  PanelLeft,
  PanelRight,
  SquarePen,
  CalendarClock,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useSidebarViewOptions } from "@/contexts/sidebar-view-options-context"
import { useTabActions } from "@/contexts/tab-context"
import { useAutomationsView } from "@/contexts/automations-view-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import {
  SidebarConversationList,
  type SidebarConversationListHandle,
} from "@/components/conversations/sidebar-conversation-list"
import { NewFolderDropdown } from "@/components/layout/new-folder-dropdown"
import { SidebarAccountSettings } from "@/components/layout/sidebar-account-settings"
import {
  SidebarNavButton,
  SidebarRailButton,
} from "@/components/layout/sidebar-nav-button"
import { Button } from "@/components/ui/button"
import { useIsMobile } from "@/hooks/use-mobile"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import { formatShortcutLabel } from "@/lib/keyboard-shortcuts"
import { cn } from "@/lib/utils"

const PRIMARY_SHORTCUT_BADGE_CLASS = cn(
  "ml-auto inline-flex h-[0.9375rem] shrink-0 items-center justify-center",
  "rounded-[0.3125rem] border border-primary-foreground/25 bg-primary-foreground/15 px-[0.25rem]",
  "font-mono text-[0.625rem] font-medium leading-none text-primary-foreground/85",
  "opacity-0 transition-opacity duration-150",
  "group-hover:opacity-100 group-focus-visible:opacity-100"
)

export function Sidebar() {
  const t = useTranslations("Folder.sidebar")
  const tTitleBar = useTranslations("Folder.folderTitleBar")
  const { isOpen, toggle } = useSidebarContext()
  const { activeFolder } = useActiveFolder()
  const { openNewConversationTab, openChatModeTab } = useTabActions()
  const { unseenFailures } = useAutomationsView()
  const { routeId, setRoute, openConversations } = useWorkbenchRoute()
  const isMac = useIsMac()
  const { shortcuts } = useShortcutSettings()
  const isMobile = useIsMobile()
  const listRef = useRef<SidebarConversationListHandle>(null)

  const { showCompleted, sortMode, sectionOrder } = useSidebarViewOptions()
  const [allExpanded, setAllExpanded] = useState(true)
  const newConversationShortcutLabel = formatShortcutLabel(
    shortcuts.new_conversation,
    isMac
  )
  const toggleExpandLabel = allExpanded
    ? t("collapseAllGroups")
    : t("expandAllGroups")
  const toggleSidebarLabel = tTitleBar("withShortcut", {
    label: tTitleBar(isOpen ? "hideSidebar" : "showSidebar"),
    shortcut: formatShortcutLabel(shortcuts.toggle_sidebar, isMac),
  })

  const handleToggleExpandAll = useCallback(() => {
    if (allExpanded) {
      listRef.current?.collapseAll()
      setAllExpanded(false)
    } else {
      listRef.current?.expandAll()
      setAllExpanded(true)
    }
  }, [allExpanded])

  const handleNewConversation = useCallback(() => {
    // Starting a conversation always returns to the conversation workspace (in
    // case a route like Automations was taking over the content region).
    openConversations()
    // Defense-in-depth: with no active folder (e.g. a cold start that recovered
    // to nothing, or all folders closed) fall back to folderless chat mode
    // rather than no-op, so this entry point is never a dead end.
    if (!activeFolder) {
      openChatModeTab()
      return
    }
    openNewConversationTab(activeFolder.id, activeFolder.path)
  }, [activeFolder, openChatModeTab, openNewConversationTab, openConversations])

  if (!isOpen) {
    if (isMobile) return null

    return (
      <aside className="flex h-full min-h-0 w-full flex-col items-center overflow-hidden border-r border-sidebar-border/70 bg-sidebar/95 py-2 text-sidebar-foreground select-none">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-primary/20 bg-primary/10 text-[0.8125rem] font-semibold text-primary">
          N
        </div>

        <div className="mt-3 flex shrink-0 flex-col items-center gap-1.5 border-t border-sidebar-border/60 pt-3">
          <SidebarRailButton
            icon={PanelRight}
            label={toggleSidebarLabel}
            onClick={toggle}
          />
          <div
            className={cn(
              "flex h-8 w-8 items-center justify-center rounded-lg",
              "text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground",
              "[&_button]:h-8 [&_button]:w-8 [&_button]:rounded-lg"
            )}
          >
            <NewFolderDropdown />
          </div>
          <SidebarRailButton
            icon={SquarePen}
            label={t("newChat")}
            onClick={handleNewConversation}
            tone="primary"
          />
        </div>

        <div className="mt-3 flex shrink-0 flex-col items-center gap-1.5 border-t border-sidebar-border/60 pt-3">
          <SidebarRailButton
            icon={CalendarClock}
            label={t("automations")}
            active={routeId === "automations"}
            onClick={() => setRoute("automations")}
          />
          <SidebarRailButton
            icon={PackageCheck}
            label={t("skillsMarket")}
            active={routeId === "skills"}
            onClick={() => setRoute("skills")}
          />
        </div>
      </aside>
    )
  }

  return (
    <aside className="@container/sidebar flex h-full min-h-0 flex-col overflow-hidden border-r border-sidebar-border/70 bg-sidebar/95 text-sidebar-foreground select-none">
      <div className="flex h-10 shrink-0 items-center justify-between border-b border-sidebar-border/70 px-3">
        <span className="text-[0.6875rem] font-semibold uppercase tracking-[0.04em] text-muted-foreground">
          {t("title")}
        </span>
        <div className="flex items-center gap-0.5">
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0 rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={handleToggleExpandAll}
            title={toggleExpandLabel}
            aria-label={toggleExpandLabel}
          >
            {allExpanded ? (
              <ChevronsDownUp aria-hidden="true" className="h-3.5 w-3.5" />
            ) : (
              <ChevronsUpDown aria-hidden="true" className="h-3.5 w-3.5" />
            )}
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0 rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground"
            onClick={toggle}
            title={toggleSidebarLabel}
            aria-label={toggleSidebarLabel}
          >
            <PanelLeft className="h-3.5 w-3.5" aria-hidden="true" />
          </Button>
        </div>
      </div>

      <div className="flex shrink-0 flex-col gap-2 border-b border-sidebar-border/60 px-3 py-2.5">
        <div className="flex min-w-0 items-center gap-2">
          <div className="min-w-0 flex-1">
            <SidebarNavButton
              icon={SquarePen}
              label={t("newChat")}
              onClick={handleNewConversation}
              tone="primary"
              trailing={
                newConversationShortcutLabel ? (
                  <kbd className={PRIMARY_SHORTCUT_BADGE_CLASS}>
                    {newConversationShortcutLabel}
                  </kbd>
                ) : null
              }
            />
          </div>
          <NewFolderDropdown buttonClassName="h-10 w-10 shrink-0 rounded-lg border border-sidebar-border/70 bg-sidebar-accent/35 text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-foreground" />
        </div>
        <div className="flex min-w-0 items-center gap-1 border-t border-sidebar-border/60 pt-2">
          <SidebarNavButton
            icon={CalendarClock}
            label={t("automations")}
            active={routeId === "automations"}
            onClick={() => setRoute("automations")}
            className="h-8 min-w-0 flex-1 gap-1.5 px-2 text-[0.8125rem]"
            trailing={
              unseenFailures > 0 ? (
                <span className="ml-auto inline-flex h-[0.9375rem] min-w-[0.9375rem] shrink-0 items-center justify-center rounded-full bg-destructive/15 px-1 font-mono text-[0.625rem] font-medium leading-none text-destructive">
                  {unseenFailures}
                </span>
              ) : null
            }
          />
          <SidebarNavButton
            icon={PackageCheck}
            label={t("skillsMarket")}
            active={routeId === "skills"}
            onClick={() => setRoute("skills")}
            className="h-8 min-w-0 flex-1 gap-1.5 px-2 text-[0.8125rem]"
          />
        </div>
      </div>

      {/* On mobile, clicking a conversation card auto-closes the Sheet */}
      <div
        className="relative flex min-h-0 flex-1 flex-col overflow-hidden"
        onClick={
          isMobile
            ? (e) => {
                const target = e.target as HTMLElement
                if (target.closest("[data-conversation-id]")) {
                  toggle()
                }
              }
            : undefined
        }
      >
        <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
          <SidebarConversationList
            ref={listRef}
            showCompleted={showCompleted}
            sortMode={sortMode}
            sectionOrder={sectionOrder}
          />
          <Button
            variant="ghost"
            size="icon"
            className={cn(
              "absolute right-3 bottom-3 z-20 h-8 w-8 rounded-full",
              "border border-sidebar-border/80 bg-sidebar/90 text-muted-foreground shadow-md shadow-black/10 backdrop-blur",
              "hover:bg-sidebar-accent hover:text-sidebar-foreground"
            )}
            onClick={(event) => {
              event.stopPropagation()
              listRef.current?.scrollToActive()
            }}
            title={t("locateActiveConversation")}
            aria-label={t("locateActiveConversation")}
          >
            <Crosshair aria-hidden="true" className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      <div className="flex shrink-0 flex-col border-t border-sidebar-border/70 bg-sidebar/95 px-2 py-1">
        <SidebarAccountSettings />
      </div>
    </aside>
  )
}
