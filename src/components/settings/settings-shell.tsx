"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ComponentType,
  type ReactNode,
} from "react"
import {
  Activity,
  ArrowLeft,
  BarChart3,
  Bot,
  Brain,
  GitBranch,
  Keyboard,
  Menu,
  MessageSquareText,
  Package,
  Palette,
  PlugZap,
  ScrollText,
  Search,
  SendHorizontal,
  Settings,
  SlidersHorizontal,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { usePathname } from "next/navigation"
import { useRouter } from "next/navigation"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { AppToaster } from "@/components/ui/app-toaster"
import { cn } from "@/lib/utils"
import { AppTitleBar } from "@/components/layout/app-title-bar"
import { useIsMobile } from "@/hooks/use-mobile"
import { Sheet, SheetContent, SheetTitle } from "@/components/ui/sheet"

export interface SettingsNavItem {
  href: string
  labelKey:
    | "general"
    | "appearance"
    | "usage"
    | "user_memory"
    | "agents"
    | "mcp"
    | "skills"
    | "quick_messages"
    | "shortcuts"
    | "version_control"
    | "chat_channels"
    | "system"
    | "logs"
    | "performance"
  icon: ComponentType<{ className?: string }>
}

export type SettingsNavGroupKey =
  | "personalization"
  | "aiModels"
  | "productivity"
  | "integrations"
  | "system"

export interface SettingsNavGroup {
  groupKey: SettingsNavGroupKey
  items: SettingsNavItem[]
}

const SHOW_RUNTIME_LOGS_SETTINGS = false
const SHOW_VERSION_CONTROL_SETTINGS = false

// ─── Nav item definitions ─────────────────────────────────────────────────────

const NAV_APPEARANCE: SettingsNavItem = {
  href: "/settings/appearance",
  labelKey: "appearance",
  icon: Palette,
}
const NAV_GENERAL: SettingsNavItem = {
  href: "/settings/general",
  labelKey: "general",
  icon: SlidersHorizontal,
}
const NAV_AGENTS: SettingsNavItem = {
  href: "/settings/agents",
  labelKey: "agents",
  icon: Bot,
}
const NAV_MCP: SettingsNavItem = {
  href: "/settings/mcp",
  labelKey: "mcp",
  icon: PlugZap,
}
const NAV_SKILLS: SettingsNavItem = {
  href: "/settings/skills",
  labelKey: "skills",
  icon: Package,
}
const NAV_QUICK_MESSAGES: SettingsNavItem = {
  href: "/settings/quick-messages",
  labelKey: "quick_messages",
  icon: MessageSquareText,
}
const NAV_SHORTCUTS: SettingsNavItem = {
  href: "/settings/shortcuts",
  labelKey: "shortcuts",
  icon: Keyboard,
}
const NAV_USER_MEMORY: SettingsNavItem = {
  href: "/settings/user-memory",
  labelKey: "user_memory",
  icon: Brain,
}
const NAV_CHAT_CHANNELS: SettingsNavItem = {
  href: "/settings/chat-channels",
  labelKey: "chat_channels",
  icon: SendHorizontal,
}
const NAV_PERFORMANCE: SettingsNavItem = {
  href: "/settings/performance",
  labelKey: "performance",
  icon: Activity,
}
const NAV_USAGE: SettingsNavItem = {
  href: "/settings/usage",
  labelKey: "usage",
  icon: BarChart3,
}
const NAV_SYSTEM: SettingsNavItem = {
  href: "/settings/system",
  labelKey: "system",
  icon: Settings,
}
const NAV_LOGS: SettingsNavItem = {
  href: "/settings/logs",
  labelKey: "logs",
  icon: ScrollText,
}
const NAV_VERSION_CONTROL: SettingsNavItem = {
  href: "/settings/version-control",
  labelKey: "version_control",
  icon: GitBranch,
}

// ─── Grouped structure (model-providers + web-service excluded from nav) ──────

const NAV_GROUPS_BASE: SettingsNavGroup[] = [
  {
    groupKey: "personalization",
    items: [NAV_APPEARANCE, NAV_GENERAL],
  },
  {
    groupKey: "aiModels",
    items: [NAV_AGENTS, NAV_MCP, NAV_SKILLS],
  },
  {
    groupKey: "productivity",
    items: [NAV_QUICK_MESSAGES, NAV_SHORTCUTS, NAV_USER_MEMORY],
  },
  {
    groupKey: "integrations",
    items: [NAV_CHAT_CHANNELS],
  },
  {
    groupKey: "system",
    items: [
      NAV_PERFORMANCE,
      NAV_USAGE,
      NAV_SYSTEM,
      ...(SHOW_RUNTIME_LOGS_SETTINGS ? [NAV_LOGS] : []),
      ...(SHOW_VERSION_CONTROL_SETTINGS ? [NAV_VERSION_CONTROL] : []),
    ],
  },
]

// Flat list kept for backward compatibility with any external consumers.
export const SETTINGS_NAV_ITEMS: SettingsNavItem[] = NAV_GROUPS_BASE.flatMap(
  (g) => g.items
)

// ─── Path normalisation ───────────────────────────────────────────────────────

export function normalizeSettingsPath(path: string): string {
  const noSuffix = path.replace(/\/index\.html$/, "").replace(/\.html$/, "")
  const noTrailingSlash = noSuffix.replace(/\/+$/, "")
  return noTrailingSlash || "/"
}

export function normalizeSettingsNavPath(path: string): string {
  const normalized = normalizeSettingsPath(path)
  switch (normalized) {
    case "/settings/experts":
    case "/settings/office-tools":
    case "/settings/internet-tools":
    case "/settings/codex-native":
      return "/settings/skills"
    default:
      return normalized
  }
}

function isWindowsRuntime(): boolean {
  if (typeof navigator === "undefined") return false
  const platform = navigator.platform.toLowerCase()
  const userAgent = navigator.userAgent.toLowerCase()
  return platform.includes("win") || userAgent.includes("windows")
}

// ─── Shell props ──────────────────────────────────────────────────────────────

interface SettingsShellProps {
  children: ReactNode
  activePath?: string
  className?: string
  onBack?: () => void
  onNavigate?: (href: string) => void
  showToaster?: boolean
  showWindowControls?: boolean
  updateDocumentTitle?: boolean
}

// ─── Shell ────────────────────────────────────────────────────────────────────

export function SettingsShell({
  children,
  activePath,
  className,
  onBack,
  onNavigate,
  showToaster = true,
  showWindowControls = true,
  updateDocumentTitle = true,
}: SettingsShellProps) {
  const t = useTranslations("SettingsShell")
  const pathname = usePathname()
  const router = useRouter()
  const normalizedPathname = normalizeSettingsNavPath(activePath ?? pathname)
  const isMobile = useIsMobile()
  const [navOpen, setNavOpen] = useState(false)
  const [search, setSearch] = useState("")

  useEffect(() => {
    if (!updateDocumentTitle) return
    document.title = `${t("title")} - iyw-claw`
  }, [t, updateDocumentTitle])

  const navigateTo = useCallback(
    (href: string) => {
      if (typeof window === "undefined") return

      const target = normalizeSettingsPath(href)
      const current = onNavigate
        ? normalizedPathname
        : normalizeSettingsPath(window.location.pathname)
      if (current === target) {
        setNavOpen(false)
        return
      }

      if (onNavigate) {
        onNavigate(target)
        setNavOpen(false)
        return
      }

      // Preserve current query string so the active remote workspace context
      // (`?remoteConnectionId=N`) carries over to sub-pages.
      const search = window.location.search
      const fullTarget = search ? `${target}${search}` : target

      if (isWindowsRuntime()) {
        window.location.assign(fullTarget)
        return
      }

      router.push(fullTarget)
      setNavOpen(false)
    },
    [normalizedPathname, onNavigate, router, setNavOpen]
  )

  // Filter groups by search query
  const filteredGroups = useMemo<SettingsNavGroup[]>(() => {
    const q = search.trim().toLowerCase()
    if (!q) return NAV_GROUPS_BASE
    return NAV_GROUPS_BASE.map((group) => ({
      ...group,
      items: group.items.filter((item) =>
        t(`nav.${item.labelKey}`).toLowerCase().includes(q)
      ),
    })).filter((group) => group.items.length > 0)
  }, [search, t])

  const navContent = (
    <div className="flex min-h-0 flex-1 flex-col gap-1">
      {/* Search bar */}
      <div className="px-2 pb-1">
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/50" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("searchPlaceholder")}
            className={cn(
              "h-7 w-full rounded-md border border-transparent bg-muted/50 pl-7 pr-3 text-xs",
              "outline-none transition-[border-color,box-shadow]",
              "placeholder:text-muted-foreground/40",
              "focus:border-border focus:bg-background focus:ring-1 focus:ring-ring/30"
            )}
          />
        </div>
      </div>

      {/* Grouped navigation */}
      <ScrollArea className="min-h-0 flex-1">
        <nav className="space-y-0.5 pb-1">
          {filteredGroups.map((group) => (
            <div key={group.groupKey}>
              {/* Group label – hidden during search */}
              {!search.trim() && (
                <div className="px-2 pb-1 pt-3 first:pt-0.5">
                  <span className="text-[10px] font-semibold uppercase tracking-widest text-muted-foreground/50">
                    {t(`navGroups.${group.groupKey}`)}
                  </span>
                </div>
              )}
              {group.items.map((item) => {
                const Icon = item.icon
                const active =
                  normalizedPathname === item.href ||
                  normalizedPathname.startsWith(`${item.href}/`)
                return (
                  <Button
                    key={item.href}
                    variant={active ? "secondary" : "ghost"}
                    size="sm"
                    className={cn(
                      "w-full justify-start px-2 transition-colors",
                      active && "font-medium"
                    )}
                    type="button"
                    onClick={() => navigateTo(item.href)}
                    aria-current={active ? "page" : undefined}
                  >
                    <span className="inline-flex items-center gap-1.5">
                      <Icon
                        className={cn(
                          "h-3.5 w-3.5 shrink-0",
                          active
                            ? "text-foreground"
                            : "text-muted-foreground/70"
                        )}
                      />
                      {t(`nav.${item.labelKey}`)}
                    </span>
                  </Button>
                )
              })}
            </div>
          ))}

          {/* Empty search state */}
          {filteredGroups.length === 0 && (
            <p className="px-2 py-6 text-center text-xs text-muted-foreground/50">
              {t("searchEmpty")}
            </p>
          )}
        </nav>
      </ScrollArea>

      {/* Back to workspace */}
      <div className="border-t pt-2">
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start px-2 text-muted-foreground hover:text-foreground"
          type="button"
          onClick={() => {
            if (onBack) {
              onBack()
              return
            }
            navigateTo("/workspace")
          }}
        >
          <span className="inline-flex items-center gap-1.5">
            <ArrowLeft className="h-3.5 w-3.5 shrink-0" />
            {t("backToWorkspace")}
          </span>
        </Button>
      </div>
    </div>
  )

  return (
    <div
      className={cn(
        "h-screen flex flex-col overflow-hidden bg-background text-foreground",
        className
      )}
    >
      <AppTitleBar
        draggable={showWindowControls}
        showWindowControls={showWindowControls}
        left={
          isMobile ? (
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={() => setNavOpen(true)}
            >
              <Menu className="h-4 w-4" />
            </Button>
          ) : undefined
        }
        center={
          <div className="text-sm font-bold tracking-tight">{t("title")}</div>
        }
      />

      <div className="flex min-h-0 flex-1">
        {/* Desktop sidebar */}
        {!isMobile && (
          <aside className="flex min-h-0 w-52 shrink-0 flex-col border-r px-2 py-3">
            {navContent}
          </aside>
        )}

        {/* Mobile navigation Sheet */}
        {isMobile && (
          <Sheet open={navOpen} onOpenChange={setNavOpen}>
            <SheetContent
              side="left"
              showCloseButton={false}
              className="w-[260px] p-3"
            >
              <SheetTitle className="sr-only">{t("title")}</SheetTitle>
              {navContent}
            </SheetContent>
          </Sheet>
        )}

        {/* Content area — keyed to trigger fade-in on navigation */}
        <section
          key={normalizedPathname}
          className="min-h-0 min-w-0 flex-1 overflow-hidden animate-in fade-in-0 slide-in-from-bottom-1 duration-150"
        >
          {children}
        </section>
      </div>

      {showToaster && (
        <AppToaster position="bottom-right" closeButton duration={4000} />
      )}
    </div>
  )
}
