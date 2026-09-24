"use client"

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react"
import {
  ChevronsDownUp,
  ChevronsUpDown,
  Crosshair,
  Folder,
  SquarePen,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useSidebarViewOptions } from "@/contexts/sidebar-view-options-context"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import {
  SidebarConversationList,
  type SidebarConversationListHandle,
} from "@/components/conversations/sidebar-conversation-list"
import {
  DEFAULT_SIDEBAR_FILTERS,
  SidebarFilterControl,
  type SidebarFilters,
} from "@/components/conversations/sidebar-filters"
import { SidebarAccountSettings } from "./sidebar-account-settings"
import { SidebarProjectActions } from "./sidebar-project-actions"
import { SidebarWorkbench } from "./sidebar-workbench"
import {
  focusSidebarToggleAfterCollapse,
  resolveSidebarPresentation,
} from "./sidebar-presentation"
import {
  SidebarNavButton,
  SidebarRailButton,
  SidebarToggleButton,
} from "./sidebar-nav-button"
import { Button } from "@/components/ui/button"
import { useIsMobile } from "@/hooks/use-mobile"
import { useZoomLevel } from "@/hooks/use-appearance"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import { formatShortcutLabel } from "@/lib/keyboard-shortcuts"
import { scalePanelPixels } from "@/lib/panel-sizing"
import { cn } from "@/lib/utils"

export function Sidebar() {
  const t = useTranslations("Folder.sidebar")
  const td = useTranslations("SidebarDesign")
  const tTitleBar = useTranslations("Folder.folderTitleBar")
  const { isOpen, toggle, width } = useSidebarContext()
  const { openChatModeTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const isMac = useIsMac()
  const { shortcuts } = useShortcutSettings()
  const isMobile = useIsMobile()
  const { zoomLevel } = useZoomLevel()
  const listRef = useRef<SidebarConversationListHandle>(null)
  const expandedLayerRef = useRef<HTMLDivElement>(null)
  const toggleButtonRef = useRef<HTMLButtonElement>(null)
  const { showCompleted, setShowCompleted, sortMode, sectionOrder } =
    useSidebarViewOptions()
  const [locateRequest, setLocateRequest] = useState(0)
  const [allExpanded, setAllExpanded] = useState(true)
  const [filters, setFilters] = useState<SidebarFilters>(
    DEFAULT_SIDEBAR_FILTERS
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
    if (!isOpen)
      focusSidebarToggleAfterCollapse(
        expandedLayerRef.current,
        toggleButtonRef.current
      )
  }, [isOpen])
  const handleNewConversation = useCallback(() => {
    openConversations()
    openChatModeTab()
    if (isMobile && isOpen) toggle()
  }, [openChatModeTab, openConversations, isMobile, isOpen, toggle])
  const handleToggleExpandAll = () => {
    if (allExpanded) listRef.current?.collapseAll()
    else listRef.current?.expandAll()
    setAllExpanded(!allExpanded)
  }
  useEffect(() => {
    if (locateRequest > 0) listRef.current?.scrollToActive()
  }, [locateRequest])
  const locateActive = () => {
    setFilters(DEFAULT_SIDEBAR_FILTERS)
    setShowCompleted(true)
    setLocateRequest((value) => value + 1)
  }
  if (!presentation.renderExpanded && !presentation.renderRail) return null
  return (
    <aside className="@container/sidebar relative h-full min-h-0 w-full overflow-hidden border-r border-sidebar-border/70 bg-sidebar text-sidebar-foreground select-none">
      <SidebarToggleButton
        ref={toggleButtonRef}
        isOpen={isOpen}
        label={toggleSidebarLabel}
        onClick={toggle}
        className="absolute right-3 top-3 z-30"
      />
      {presentation.renderExpanded && (
        <div
          ref={expandedLayerRef}
          inert={!presentation.expandedInteractive || undefined}
          aria-hidden={!presentation.expandedInteractive}
          data-open={isOpen}
          style={
            isMobile
              ? undefined
              : ({
                  "--sidebar-expanded-width": expandedLayerWidth + "px",
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
          <div className="flex h-14 shrink-0 items-center gap-2 px-4 pr-12">
            <span className="flex size-7 shrink-0 items-center justify-center rounded-md border bg-background">
              <Folder className="size-3.5" />
            </span>
            <span className="truncate text-xs font-semibold">
              {td("workspace")}
            </span>
          </div>
          <div className="grid shrink-0 gap-2 px-3 pb-4 pt-1">
            <SidebarNavButton
              icon={SquarePen}
              label={t("newChat")}
              onClick={handleNewConversation}
              tone="primary"
            />
            <SidebarProjectActions />
          </div>
          <SidebarWorkbench />
          <div className="flex h-10 shrink-0 items-center justify-between px-3">
            <span className="text-xs font-medium">{t("title")}</span>
            <div className="flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="size-7 text-muted-foreground"
                title={toggleExpandLabel}
                aria-label={toggleExpandLabel}
                onClick={handleToggleExpandAll}
              >
                {allExpanded ? (
                  <ChevronsDownUp className="size-3.5" />
                ) : (
                  <ChevronsUpDown className="size-3.5" />
                )}
              </Button>
              <SidebarFilterControl value={filters} onChange={setFilters} />
            </div>
          </div>
          <div
            className="relative flex min-h-0 flex-1 flex-col overflow-hidden"
            onClick={
              isMobile
                ? (event) => {
                    if (
                      (event.target as HTMLElement).closest(
                        "[data-conversation-id]"
                      )
                    )
                      toggle()
                  }
                : undefined
            }
          >
            <SidebarConversationList
              ref={listRef}
              showCompleted={filters.status === "all" ? showCompleted : true}
              sortMode={sortMode}
              sectionOrder={sectionOrder}
              filters={filters}
            />
            <Button
              variant="ghost"
              size="icon"
              className="absolute bottom-3 right-3 z-20 size-8 rounded-md border border-sidebar-border/80 bg-sidebar/95 text-muted-foreground shadow-sm hover:bg-sidebar-accent"
              onClick={locateActive}
              title={t("locateActiveConversation")}
              aria-label={t("locateActiveConversation")}
            >
              <Crosshair className="size-3.5" />
            </Button>
          </div>
          <div className="shrink-0 border-t border-sidebar-border/70 px-2 py-1">
            <SidebarAccountSettings />
          </div>
        </div>
      )}
      {presentation.renderRail && (
        <div
          inert={!presentation.railInteractive || undefined}
          aria-hidden={!presentation.railInteractive}
          className={cn(
            "absolute inset-y-0 left-0 flex w-full flex-col items-center bg-sidebar transition-opacity duration-150 motion-reduce:transition-none",
            presentation.railInteractive
              ? "opacity-100"
              : "pointer-events-none opacity-0"
          )}
        >
          <div className="h-14 w-full shrink-0" />
          <div className="flex shrink-0 flex-col items-center gap-2 py-2">
            <SidebarRailButton
              icon={SquarePen}
              label={t("newChat")}
              onClick={handleNewConversation}
              tone="primary"
            />
            <SidebarProjectActions compact />
          </div>
          <SidebarWorkbench compact />
          <div className="mt-auto w-full border-t border-sidebar-border/70 py-2">
            <SidebarAccountSettings compact />
          </div>
        </div>
      )}
    </aside>
  )
}
