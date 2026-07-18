"use client"

import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react"
import {
  ChevronsDownUp,
  ChevronsUpDown,
  Crosshair,
  PackageCheck,
  Settings,
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
  focusSidebarToggleAfterCollapse,
  resolveSidebarPresentation,
} from "@/components/layout/sidebar-presentation"
import {
  SidebarNavButton,
  SidebarRailButton,
  SidebarToggleButton,
} from "@/components/layout/sidebar-nav-button"
import { Button } from "@/components/ui/button"
import { useIsMobile } from "@/hooks/use-mobile"
import { useZoomLevel } from "@/hooks/use-appearance"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import { formatShortcutLabel } from "@/lib/keyboard-shortcuts"
import { openSettingsWindow } from "@/lib/api"
import { scalePanelPixels } from "@/lib/panel-sizing"
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
  const { isOpen, toggle, width } = useSidebarContext()
  const { activeFolder } = useActiveFolder()
  const { openNewConversationTab, openChatModeTab } = useTabActions()
  const { unseenFailures } = useAutomationsView()
  const { routeId, setRoute, openConversations } = useWorkbenchRoute()
  const isMac = useIsMac()
  const { shortcuts } = useShortcutSettings()
  const isMobile = useIsMobile()
  const { zoomLevel } = useZoomLevel()
  const listRef = useRef<SidebarConversationListHandle>(null)
  const expandedLayerRef = useRef<HTMLDivElement>(null)
  const toggleButtonRef = useRef<HTMLButtonElement>(null)

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
  const presentation = resolveSidebarPresentation(isOpen, isMobile)
  const expandedLayerWidth = scalePanelPixels(width, zoomLevel)

  useLayoutEffect(() => {
    if (!isOpen) {
      focusSidebarToggleAfterCollapse(
        expandedLayerRef.current,
        toggleButtonRef.current
      )
    }
  }, [isOpen])

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
    // A new conversation always returns to the conversation workspace.
    openConversations()
    // Keep this entry point useful when no folder is active.
    if (!activeFolder) {
      openChatModeTab()
      return
    }
    openNewConversationTab(activeFolder.id, activeFolder.path)
  }, [activeFolder, openChatModeTab, openNewConversationTab, openConversations])

  const handleOpenSettings = useCallback(() => {
    openSettingsWindow("appearance").catch((error) => {
      console.error("[Sidebar] failed to open settings:", error)
    })
  }, [])

  if (!presentation.renderExpanded && !presentation.renderRail) return null

  return (
    <aside className="@container/sidebar relative h-full min-h-0 w-full overflow-hidden border-r border-sidebar-border/70 bg-sidebar text-sidebar-foreground select-none">
      <SidebarToggleButton
        ref={toggleButtonRef}
        isOpen={isOpen}
        label={toggleSidebarLabel}
        onClick={toggle}
        className="absolute top-2 right-3 z-30"
      />
      {presentation.renderExpanded ? (
        <div
          ref={expandedLayerRef}
          inert={!presentation.expandedInteractive || undefined}
          aria-hidden={!presentation.expandedInteractive}
          data-open={isOpen}
          style={
            isMobile
              ? undefined
              : ({
                  "--sidebar-expanded-width": `${expandedLayerWidth}px`,
                } as CSSProperties)
          }
          className={cn(
            "flex h-full min-h-0 flex-col overflow-hidden bg-sidebar",
            !isMobile &&
              "sidebar-expanded-layer absolute inset-y-0 left-0 transition-[opacity,transform] duration-150 ease-out motion-reduce:transition-none",
            !isMobile &&
              (presentation.expandedInteractive
                ? "translate-x-0 opacity-100"
                : "pointer-events-none -translate-x-1 opacity-0")
          )}
        >
          <div className="flex h-11 shrink-0 items-center justify-between border-b border-sidebar-border/70 px-3">
            <span className="text-xs font-semibold text-sidebar-foreground">
              {t("title")}
            </span>
            <div className="mr-8 flex items-center">
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
            </div>
          </div>

          <div className="grid shrink-0 gap-1.5 border-b border-sidebar-border/60 px-3 py-3">
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
            <NewFolderDropdown
              showLabel
              buttonClassName="h-9 w-full justify-start rounded-md border border-sidebar-border/80 bg-sidebar text-sidebar-foreground/75 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            />
          </div>

          <nav className="grid shrink-0 gap-0.5 border-b border-sidebar-border/60 px-2 py-2">
            <SidebarNavButton
              icon={CalendarClock}
              label={t("automations")}
              active={routeId === "automations"}
              onClick={() => setRoute("automations")}
              trailing={
                unseenFailures > 0 ? (
                  <span className="ml-auto inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full bg-destructive/15 px-1 font-mono text-[0.625rem] font-medium leading-none text-destructive">
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
            />
          </nav>

          <div
            className="relative flex min-h-0 flex-1 flex-col overflow-hidden"
            onClick={
              isMobile
                ? (event) => {
                    const target = event.target as HTMLElement
                    if (target.closest("[data-conversation-id]")) toggle()
                  }
                : undefined
            }
          >
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

          <div className="shrink-0 border-t border-sidebar-border/70 px-2 py-1">
            <SidebarAccountSettings />
          </div>
        </div>
      ) : null}

      {presentation.renderRail ? (
        <div
          inert={!presentation.railInteractive || undefined}
          aria-hidden={!presentation.railInteractive}
          className={cn(
            "absolute inset-y-0 left-0 flex w-full flex-col items-center bg-sidebar",
            "transition-opacity duration-150 ease-out motion-reduce:transition-none",
            presentation.railInteractive
              ? "opacity-100"
              : "pointer-events-none opacity-0"
          )}
        >
          <div className="h-11 w-full shrink-0 border-b border-sidebar-border/70" />
          <div className="flex shrink-0 flex-col items-center gap-1.5 py-2">
            <SidebarRailButton
              icon={SquarePen}
              label={t("newChat")}
              onClick={handleNewConversation}
              tone="primary"
            />
            <NewFolderDropdown buttonClassName="h-9 w-9 rounded-md text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground" />
          </div>
          <div className="h-px w-7 shrink-0 bg-sidebar-border/70" />
          <nav className="flex shrink-0 flex-col items-center gap-1.5 py-2">
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
          </nav>
          <div className="mt-auto flex w-full justify-center border-t border-sidebar-border/70 py-2">
            <SidebarRailButton
              icon={Settings}
              label={tTitleBar("openSettings")}
              onClick={handleOpenSettings}
            />
          </div>
        </div>
      ) : null}
    </aside>
  )
}
