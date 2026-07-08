"use client"

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react"
import {
  ChevronsDownUp,
  ChevronsUpDown,
  Crosshair,
  FolderOpenDot,
  Funnel,
  PackageCheck,
  SquarePen,
  Zap,
  type LucideIcon,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useTabActions } from "@/contexts/tab-context"
import { useAutomationsView } from "@/contexts/automations-view-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import {
  SidebarConversationList,
  type SidebarConversationListHandle,
} from "@/components/conversations/sidebar-conversation-list"
import { SidebarAccountSettings } from "@/components/layout/sidebar-account-settings"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useIsMobile } from "@/hooks/use-mobile"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import { formatShortcutLabel } from "@/lib/keyboard-shortcuts"
import {
  loadShowCompleted,
  loadSortMode,
  loadSectionOrder,
  saveShowCompleted,
  saveSortMode,
  saveSectionOrder,
  type SidebarSortMode,
  type SidebarSectionOrder,
} from "@/lib/sidebar-view-mode-storage"
import { cn } from "@/lib/utils"

function getFolderName(path: string | undefined) {
  if (!path) return ""
  const normalized = path.replace(/\\/g, "/")
  return normalized.split("/").filter(Boolean).pop() ?? normalized
}

// Keyboard-shortcut hint at the trailing edge of fixed sidebar rows.
// Mirrors the folder count badge exactly — same chip (0.9375rem height,
// 0.3125rem radius, bg-primary/10, text-primary, 0.625rem text) per the request
// to match it. That pairing is also solidly legible (text-primary on
// primary/10 ≈ 14:1 light / 11:1 dark), unlike the muted-on-muted kbd it
// replaces (4.34:1). Revealed only on hover / keyboard focus of its row (each
// row is a `group`); font-mono renders the shortcut glyphs cleanly.
const SHORTCUT_BADGE_CLASS = cn(
  "ml-auto inline-flex h-[0.9375rem] shrink-0 items-center justify-center",
  "rounded-[0.3125rem] border border-sidebar-border/60 bg-sidebar-accent/60 px-[0.25rem]",
  "font-mono text-[0.625rem] font-medium leading-none text-muted-foreground",
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
        "group flex w-full items-center gap-2.5 rounded-xl px-3",
        "text-[0.875rem] outline-none",
        "transition-colors duration-150 hover:bg-sidebar-accent",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
        isPrimary
          ? "h-10 bg-sidebar-foreground text-sidebar hover:bg-sidebar-foreground/90"
          : "h-9 text-sidebar-foreground/80",
        active && "bg-sidebar-accent text-sidebar-foreground"
      )}
    >
      <Icon
        className={cn(
          "h-[0.875rem] w-[0.875rem] shrink-0",
          isPrimary ? "text-sidebar" : "text-muted-foreground"
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

  const [showCompleted, setShowCompleted] = useState(false)
  const [sortMode, setSortMode] = useState<SidebarSortMode>("created")
  const [sectionOrder, setSectionOrder] =
    useState<SidebarSectionOrder>("folders-first")
  const [allExpanded, setAllExpanded] = useState(true)
  const newConversationShortcutLabel = formatShortcutLabel(
    shortcuts.new_conversation,
    isMac
  )
  // General umbrella name for the funnel menu (show-completed + sort + section
  // order). Kept generic so the accessible name / tooltip stays accurate as the
  // menu gains options.
  const viewOptionsLabel = t("viewOptions")
  const toggleExpandLabel = allExpanded
    ? t("collapseAllGroups")
    : t("expandAllGroups")
  const activeFolderName = getFolderName(activeFolder?.path)

  useEffect(() => {
    // Hydrate from localStorage after mount to keep SSR/CSR markup consistent.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setShowCompleted(loadShowCompleted())
    setSortMode(loadSortMode())
    setSectionOrder(loadSectionOrder())
  }, [])

  const handleSetShowCompleted = useCallback((value: boolean) => {
    setShowCompleted(value)
    saveShowCompleted(value)
  }, [])

  const handleSetSortMode = useCallback((value: string) => {
    const mode: SidebarSortMode = value === "updated" ? "updated" : "created"
    setSortMode(mode)
    saveSortMode(mode)
  }, [])

  const handleSetSectionOrder = useCallback((value: string) => {
    const next: SidebarSectionOrder =
      value === "chats-first" ? "chats-first" : "folders-first"
    setSectionOrder(next)
    saveSectionOrder(next)
  }, [])

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
    <aside className="@container/sidebar flex h-full min-h-0 flex-col overflow-hidden border-r border-sidebar-border/70 bg-sidebar text-sidebar-foreground select-none">
      <div className="shrink-0 border-b border-sidebar-border/70 px-3 pb-3 pt-3">
        <div className="flex min-w-0 items-center gap-2.5">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-sidebar-foreground text-[0.8125rem] font-semibold text-sidebar shadow-sm">
            N
          </div>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[0.8125rem] font-semibold text-sidebar-foreground">
              iyw-claw
            </div>
            <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
              <FolderOpenDot className="h-3 w-3 shrink-0" aria-hidden />
              <span className="truncate">{activeFolderName || t("title")}</span>
            </div>
          </div>
        </div>
        <div className="mt-3 grid grid-cols-3 gap-1 rounded-xl bg-sidebar-accent/45 p-1">
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-full shrink-0 rounded-lg text-muted-foreground hover:bg-sidebar hover:text-sidebar-foreground"
            onClick={() => listRef.current?.scrollToActive()}
            title={t("locateActiveConversation")}
            aria-label={t("locateActiveConversation")}
          >
            <Crosshair aria-hidden="true" className="h-3.5 w-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-full shrink-0 rounded-lg text-muted-foreground hover:bg-sidebar hover:text-sidebar-foreground"
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
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-full shrink-0 rounded-lg text-muted-foreground hover:bg-sidebar hover:text-sidebar-foreground"
                title={viewOptionsLabel}
                aria-label={viewOptionsLabel}
              >
                <Funnel aria-hidden="true" className="h-3.5 w-3.5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuCheckboxItem
                checked={showCompleted}
                onCheckedChange={handleSetShowCompleted}
              >
                {t("showCompleted")}
              </DropdownMenuCheckboxItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>{t("sortBy")}</DropdownMenuLabel>
              <DropdownMenuRadioGroup
                value={sortMode}
                onValueChange={handleSetSortMode}
              >
                <DropdownMenuRadioItem value="created">
                  {t("sortByCreatedAt")}
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="updated">
                  {t("sortByUpdatedAt")}
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>{t("sectionOrder")}</DropdownMenuLabel>
              <DropdownMenuRadioGroup
                value={sectionOrder}
                onValueChange={handleSetSectionOrder}
              >
                <DropdownMenuRadioItem value="folders-first">
                  {t("sectionOrderFoldersFirst")}
                </DropdownMenuRadioItem>
                <DropdownMenuRadioItem value="chats-first">
                  {t("sectionOrderChatsFirst")}
                </DropdownMenuRadioItem>
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
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
              <kbd className={SHORTCUT_BADGE_CLASS}>
                {newConversationShortcutLabel}
              </kbd>
            ) : null
          }
        />
        <div className="pt-1">
          <div className="mb-1.5 px-1 text-[0.625rem] font-semibold uppercase tracking-[0.06em] text-muted-foreground">
            {t("title")}
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
        className="flex flex-col flex-1 min-h-0 overflow-hidden"
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
      </div>

      <div className="flex shrink-0 flex-col border-t border-sidebar-border/70 bg-sidebar/95 px-2 py-2">
        <SidebarAccountSettings />
      </div>
    </aside>
  )
}
