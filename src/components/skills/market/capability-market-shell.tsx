"use client"

import type { ReactNode } from "react"
import { Blocks, PackageOpen, PlugZap, Sparkles } from "lucide-react"
import { useTranslations } from "next-intl"
import { cn } from "@/lib/utils"

export type CapabilityMarketSection = "skills" | "connectors" | "plugins"

const SECTIONS = [
  { id: "skills" as const, icon: Sparkles },
  { id: "connectors" as const, icon: PlugZap },
  { id: "plugins" as const, icon: Blocks },
]

interface CapabilityMarketShellProps {
  activeSection: CapabilityMarketSection
  onSectionChange: (section: CapabilityMarketSection) => void
  children: ReactNode
}

function MarketIdentity() {
  const t = useTranslations("CapabilityMarket")
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-foreground text-background">
        <PackageOpen className="size-3.5" aria-hidden="true" />
      </span>
      <div className="hidden min-w-0 sm:block">
        <h1 className="truncate text-sm font-semibold leading-5">
          {t("title")}
        </h1>
        <p className="truncate text-[10px] leading-4 text-muted-foreground">
          {t("subtitle")}
        </p>
      </div>
    </div>
  )
}

function MarketNavigation({
  activeSection,
  onSectionChange,
}: Omit<CapabilityMarketShellProps, "children">) {
  const t = useTranslations("CapabilityMarket")
  return (
    <nav
      className="flex min-w-0 flex-1 items-center justify-start gap-1 overflow-x-auto overflow-y-hidden pl-1 [scrollbar-width:none] sm:pl-5 lg:pl-10 [&::-webkit-scrollbar]:hidden"
      aria-label={t("navLabel")}
    >
      {SECTIONS.map(({ id, icon: Icon }) => {
        const active = activeSection === id
        return (
          <button
            key={id}
            type="button"
            className={cn(
              "relative flex h-9 flex-none items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors",
              active
                ? "bg-muted text-foreground"
                : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
            )}
            onClick={() => onSectionChange(id)}
            aria-current={active ? "page" : undefined}
          >
            <Icon className="size-4" aria-hidden="true" />
            <span>{t(`sections.${id}`)}</span>
            {id === "plugins" ? <PreviewBadge /> : null}
          </button>
        )
      })}
    </nav>
  )
}

function PreviewBadge() {
  const t = useTranslations("CapabilityMarket")
  return (
    <span className="rounded bg-background px-1.5 py-0.5 text-[9px] font-medium text-muted-foreground shadow-sm">
      {t("preview")}
    </span>
  )
}

export function CapabilityMarketShell({
  activeSection,
  onSectionChange,
  children,
}: CapabilityMarketShellProps) {
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <header className="shrink-0 border-b bg-background/95 px-4 backdrop-blur-sm sm:px-6">
        <div className="flex min-h-14 min-w-0 items-center gap-3">
          <MarketIdentity />
          <MarketNavigation
            activeSection={activeSection}
            onSectionChange={onSectionChange}
          />
        </div>
      </header>
      <main className="min-h-0 flex-1 overflow-hidden">{children}</main>
    </div>
  )
}
