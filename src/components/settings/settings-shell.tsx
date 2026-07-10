"use client"

import {
  useCallback,
  useEffect,
  useState,
  type ComponentType,
  type ReactNode,
} from "react"
import {
  ArrowLeft,
  Bot,
  FileStack,
  GitBranch,
  Keyboard,
  Menu,
  MessageSquareText,
  SendHorizontal,
  Palette,
  PlugZap,
  Settings,
  SlidersHorizontal,
  Sparkles,
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
    | "agents"
    | "mcp"
    | "experts"
    | "office_tools"
    | "quick_messages"
    | "shortcuts"
    | "version_control"
    | "chat_channels"
    | "system"
  icon: ComponentType<{ className?: string }>
}

export const SETTINGS_NAV_ITEMS: SettingsNavItem[] = [
  {
    href: "/settings/appearance",
    labelKey: "appearance",
    icon: Palette,
  },
  {
    href: "/settings/general",
    labelKey: "general",
    icon: SlidersHorizontal,
  },
  {
    href: "/settings/mcp",
    labelKey: "mcp",
    icon: PlugZap,
  },
  {
    href: "/settings/experts",
    labelKey: "experts",
    icon: Sparkles,
  },
  {
    href: "/settings/office-tools",
    labelKey: "office_tools",
    icon: FileStack,
  },
  {
    href: "/settings/agents",
    labelKey: "agents",
    icon: Bot,
  },
  {
    href: "/settings/quick-messages",
    labelKey: "quick_messages",
    icon: MessageSquareText,
  },
  {
    href: "/settings/shortcuts",
    labelKey: "shortcuts",
    icon: Keyboard,
  },
  {
    href: "/settings/version-control",
    labelKey: "version_control",
    icon: GitBranch,
  },
  {
    href: "/settings/chat-channels",
    labelKey: "chat_channels",
    icon: SendHorizontal,
  },
  {
    href: "/settings/system",
    labelKey: "system",
    icon: Settings,
  },
]

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

export function normalizeSettingsPath(path: string): string {
  const noSuffix = path.replace(/\/index\.html$/, "").replace(/\.html$/, "")
  const noTrailingSlash = noSuffix.replace(/\/+$/, "")
  return noTrailingSlash || "/"
}

function isWindowsRuntime(): boolean {
  if (typeof navigator === "undefined") return false
  const platform = navigator.platform.toLowerCase()
  const userAgent = navigator.userAgent.toLowerCase()
  return platform.includes("win") || userAgent.includes("windows")
}

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
  const normalizedPathname = normalizeSettingsPath(activePath ?? pathname)
  const isMobile = useIsMobile()
  const [navOpen, setNavOpen] = useState(false)

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
      // (`?remoteConnectionId=N`) carries over to sub-pages — without this,
      // navigating from /settings/appearance to /settings/mcp drops the
      // remote id and the next page falls back to the local Tauri backend.
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

  const navContent = (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="px-2 pb-2 text-[11px] font-medium text-muted-foreground">
        {t("preferences")}
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <nav className="space-y-1">
          {SETTINGS_NAV_ITEMS.map((item) => {
            const Icon = item.icon
            const translationKey = `nav.${item.labelKey}` as const
            const active =
              normalizedPathname === item.href ||
              normalizedPathname.startsWith(`${item.href}/`)
            return (
              <Button
                key={item.href}
                variant={active ? "secondary" : "ghost"}
                size="sm"
                className={cn("w-full justify-start px-2")}
                type="button"
                onClick={() => navigateTo(item.href)}
                aria-current={active ? "page" : undefined}
              >
                <span className="inline-flex items-center gap-1">
                  <Icon className="h-3.5 w-3.5" />
                  {t(translationKey)}
                </span>
              </Button>
            )
          })}
        </nav>
      </ScrollArea>
      <div className="mt-2 border-t pt-2">
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start px-2"
          type="button"
          onClick={() => {
            if (onBack) {
              onBack()
              return
            }
            navigateTo("/workspace")
          }}
        >
          <span className="inline-flex items-center gap-1">
            <ArrowLeft className="h-3.5 w-3.5" />
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

      <div className="flex-1 min-h-0 flex">
        {/* Desktop sidebar */}
        {!isMobile && (
          <aside className="flex min-h-0 w-56 shrink-0 flex-col border-r px-2 py-3">
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

        <section className="flex-1 min-w-0 min-h-0 overflow-hidden">
          {children}
        </section>
      </div>
      {showToaster && (
        <AppToaster position="bottom-right" closeButton duration={4000} />
      )}
    </div>
  )
}
