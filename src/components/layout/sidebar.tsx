"use client"

import { useCallback, useRef, useState, type ReactNode } from "react"
import {
  ChevronsDownUp,
  ChevronsUpDown,
  Crosshair,
  FolderOpenDot,
  PackageCheck,
  SquarePen,
  Zap,
  type LucideIcon,
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
import { SidebarAccountSettings } from "@/components/layout/sidebar-account-settings"
import { Button } from "@/components/ui/button"
import { useIsMobile } from "@/hooks/use-mobile"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import { formatShortcutLabel } from "@/lib/keyboard-shortcuts"
import { cn } from "@/lib/utils"

function getFolderName(path: string | undefined) {
  if (!path) return ""
  const normalized = path.replace(/\\/g, "/")
  return normalized.split("/").filter(Boolean).pop() ?? normalized
}

const PRIMARY_SHORTCUT_BADGE_CLASS = cn(
  "ml-auto inline-flex h-[0.9375rem] shrink-0 items-center justify-center",
  "rounded-[0.3125rem] border border-primary-foreground/25 bg-primary-foreground/15 px-[0.25rem]",
  "font-mono text-[0.625rem] font-medium leading-none text-primary-foreground/85",
  "opacity-0 transition-opacity duration-150",
  "group-hover:opacity-100 group-focus-visible:opacity-100"
)

/**
 * A fixed top-of-sidebar action / route row. `active` marks the row as the
 * current workbench route (selected styling); `trailing` carries a shortcut hint
 * or a count badge. Extracting this keeps every fixed nav item — and any future
 * route — on one geometry instead of copy-pasting the className. Each row is a
 * `group` so a `group-hover`-revealed trailing element works.
 */
function SidebarNavButton({
  icon: Icon,
  label,
  onClick,
  active,
  trailing,
  tone = "default",
}: {
  icon: LucideIcon
  label: string
  onClick: () => void
  active?: boolean
  trailing?: ReactNode
  tone?: "default" | "primary"
}) {
  const isPrimary = tone === "primary"

  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-current={active ? "page" : undefined}
      className={cn(
        "group relative flex w-full items-center gap-2.5 rounded-lg px-3",
        "text-[0.875rem] outline-none",
        "transition-colors duration-150",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
        isPrimary
          ? "h-10 bg-primary text-primary-foreground shadow-sm hover:bg-primary/90"
          : "h-9 text-sidebar-foreground/80",
        !isPrimary &&
          "hover:bg-sidebar-accent/75 hover:text-sidebar-foreground",
        active &&
          !isPrimary &&
          "bg-sidebar-accent text-sidebar-foreground before:absolute before:left-0 before:top-2 before:bottom-2 before:w-0.5 before:rounded-full before:bg-primary"
      )}
    >
      <Icon
        className={cn(
          "h-[0.875rem] w-[0.875rem] shrink-0",
          isPrimary ? "text-primary-foreground" : "text-muted-foreground",
          active && !isPrimary && "text-primary"
        )}
      />
      <span className={cn("truncate", isPrimary && "font-medium")}>
        {label}
      </span>
      {trailing}
    </button>
  )
}

export function Sidebar() {
  const t = useTranslations("Folder.sidebar")
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
  const activeFolderName = getFolderName(activeFolder?.path)

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

  if (!isOpen) return null

  return (
    <aside className="@container/sidebar flex h-full min-h-0 flex-col overflow-hidden border-r border-sidebar-border/70 bg-sidebar/95 text-sidebar-foreground select-none">
      <div className="shrink-0 border-b border-sidebar-border/70 p-3">
        <div className="rounded-xl border border-sidebar-border/70 bg-sidebar-accent/35 p-2.5 shadow-sm shadow-black/[0.02]">
          <div className="flex min-w-0 items-center gap-2.5">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-primary/20 bg-primary/10 text-[0.8125rem] font-semibold text-primary">
              N
            </div>
            <div className="min-w-0 flex-1">
              <div className="truncate text-[0.8125rem] font-semibold text-sidebar-foreground">
                iyw-claw
              </div>
              <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
                <FolderOpenDot className="h-3 w-3 shrink-0" aria-hidden />
                <span className="truncate">
                  {activeFolderName || t("title")}
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div className="flex shrink-0 flex-col gap-2 border-b border-sidebar-border/60 px-3 py-3">
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
        <div className="rounded-xl border border-sidebar-border/70 bg-sidebar-accent/25 p-1">
          <div className="mb-1 flex items-center justify-between gap-2 px-2 py-1">
            <div className="text-[0.625rem] font-semibold text-muted-foreground">
              {t("title")}
            </div>
            <div className="flex items-center gap-0.5">
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6 shrink-0 rounded-md text-muted-foreground hover:bg-sidebar hover:text-sidebar-foreground"
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
          <SidebarNavButton
            icon={Zap}
            label={t("automations")}
            active={routeId === "automations"}
            onClick={() => setRoute("automations")}
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
          />
        </div>
      </div>

      {/* On mobile, clicking a conversation card auto-closes the Sheet */}
      <div
        className="relative flex flex-col flex-1 min-h-0 overflow-hidden"
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

      <div className="flex shrink-0 flex-col border-t border-sidebar-border/70 bg-sidebar/95 px-2 py-2">
        <SidebarAccountSettings />
      </div>
    </aside>
  )
}
