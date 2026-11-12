"use client"
import type { ReactNode } from "react"
import { ChevronRight, Plus } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { MarketViewAction } from "./market-view-action"
export type CapabilityMarketSection =
  | "all"
  | "skills"
  | "connectors"
  | "plugins"
const SECTIONS: CapabilityMarketSection[] = [
  "all",
  "skills",
  "connectors",
  "plugins",
]
interface Props {
  activeSection: CapabilityMarketSection
  onSectionChange: (section: CapabilityMarketSection) => void
  onAction: (kind: MarketViewAction["kind"]) => void
  children: ReactNode
}
export function CapabilityMarketShell(props: Props) {
  const t = useTranslations("CapabilityMarket")
  const c = useTranslations("CapabilityMarket.catalog")
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-foreground text-background">
        <PackageOpen className="size-3.5" aria-hidden="true" />
      </span>
      <div className="min-w-0">
        <h1 className="truncate text-sm font-semibold leading-5">
          <span className="sm:hidden">{t("shortTitle")}</span>
          <span className="hidden sm:inline">{t("title")}</span>
        </h1>
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
              "relative flex h-9 flex-none items-center gap-1 rounded-md px-2 text-xs font-medium transition-colors sm:gap-2 sm:px-3 sm:text-sm",
              active
                ? "bg-muted text-foreground"
                : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
            )}
            onClick={() => onSectionChange(id)}
            aria-current={active ? "page" : undefined}
          >
            <Icon className="size-3.5 sm:size-4" aria-hidden="true" />
            <span>{t(`sections.${id}`)}</span>
          </button>
        </div>
        {props.children}
      </main>
    </div>
  )
}
