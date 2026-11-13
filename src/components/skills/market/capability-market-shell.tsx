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
    <div className="h-full min-h-0 bg-[#f3f5f6] p-2 dark:bg-[#171717] max-sm:p-0">
      <main
        data-market-scroll
        className="@container h-full overflow-y-auto rounded-2xl border bg-background px-[30px] pt-[26px] pb-5 max-sm:rounded-none max-sm:border-x-0 max-sm:px-[18px] max-sm:pt-[22px] [&_button]:rounded-md"
      >
        <header className="mb-[18px] flex flex-wrap items-center justify-between gap-6 max-sm:gap-4">
          <div>
            <h1 className="text-2xl leading-[1.4] font-semibold tracking-[-0.6px] max-sm:text-[22px]">
              {t("title")}
            </h1>
            <p className="mt-[7px] text-[13px] text-muted-foreground">
              {t("subtitle")}
            </p>
          </div>
          <div className="flex items-center gap-2 max-sm:w-full">
            <Button
              variant="outline"
              className="min-h-9 rounded-md max-sm:min-h-11"
              onClick={() => props.onAction("mine")}
            >
              {c("myUploads")}
            </Button>
            <Button
              className="min-h-9 rounded-md bg-foreground text-background hover:bg-foreground/90 max-sm:min-h-11"
              onClick={() => props.onAction("upload")}
            >
              <Plus className="size-4" />
              {c("createUpload")}
            </Button>
          </div>
        </header>
        <div className="flex items-center justify-between gap-5 border-b">
          <nav
            aria-label={t("navLabel")}
            className="flex min-w-0 gap-6 overflow-x-auto overflow-y-hidden"
          >
            {SECTIONS.map((section) => (
              <button
                key={section}
                type="button"
                onClick={() => props.onSectionChange(section)}
                aria-current={
                  section === props.activeSection ? "page" : undefined
                }
                className={cn(
                  "min-h-11 shrink-0 !rounded-none border-b-2 px-0 pt-2.5 pb-[13px] text-sm",
                  section === props.activeSection
                    ? "border-foreground font-medium text-foreground"
                    : "border-transparent text-muted-foreground"
                )}
              >
                {t(`sections.${section}`)}
              </button>
            ))}
          </nav>
          <button
            className="flex min-h-11 shrink-0 items-center gap-1 text-[13px] text-muted-foreground"
            onClick={() => props.onAction("installed")}
          >
            {c("installed")}
            <ChevronRight className="size-4" />
          </button>
        </div>
        {props.children}
      </main>
    </div>
  )
}
