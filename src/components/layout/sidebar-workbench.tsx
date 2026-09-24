"use client"

import { useId, useState } from "react"
import {
  CalendarClock,
  ChevronRight,
  LibraryBig,
  PackageCheck,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useAutomationsView } from "@/contexts/automations-view-context"
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useIsMobile } from "@/hooks/use-mobile"
import { SidebarNavButton, SidebarRailButton } from "./sidebar-nav-button"
import { cn } from "@/lib/utils"

export function SidebarWorkbench({ compact = false }: { compact?: boolean }) {
  const t = useTranslations("Folder.sidebar")
  const td = useTranslations("SidebarDesign")
  const { routeId, setRoute } = useWorkbenchRoute()
  const { unseenFailures } = useAutomationsView()
  const { toggle } = useSidebarContext()
  const mobile = useIsMobile()
  const [expanded, setExpanded] = useState(true)
  const navId = useId()
  const entries = [
    { id: "automations", label: t("automations"), icon: CalendarClock },
    { id: "skills", label: t("skillsMarket"), icon: PackageCheck },
    { id: "resources", label: t("resources"), icon: LibraryBig },
  ] as const
  const openRoute = (id: (typeof entries)[number]["id"]) => {
    setRoute(id)
    if (mobile) toggle()
  }
  return (
    <section
      className={cn(
        "shrink-0 border-b border-sidebar-border/60",
        compact ? "py-2" : "px-2 pb-3"
      )}
    >
      {!compact && (
        <button
          type="button"
          aria-expanded={expanded}
          aria-controls={navId}
          onClick={() => setExpanded(!expanded)}
          className="flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-xs text-muted-foreground hover:bg-sidebar-accent focus-visible:outline focus-visible:outline-ring"
        >
          <ChevronRight
            className={cn(
              "size-3 transition-transform",
              expanded && "rotate-90"
            )}
          />
          {td("workbench")}
        </button>
      )}
      <nav
        id={navId}
        aria-label={td("workbench")}
        hidden={!compact && !expanded}
        className={cn(
          compact ? "flex flex-col items-center gap-1.5" : "space-y-0.5"
        )}
      >
        {entries.map((entry) =>
          compact ? (
            <SidebarRailButton
              key={entry.id}
              icon={entry.icon}
              label={entry.label}
              active={routeId === entry.id}
              onClick={() => openRoute(entry.id)}
            />
          ) : (
            <SidebarNavButton
              key={entry.id}
              icon={entry.icon}
              label={entry.label}
              active={routeId === entry.id}
              onClick={() => openRoute(entry.id)}
              trailing={
                entry.id === "automations" && unseenFailures > 0 ? (
                  <span className="ml-auto rounded bg-destructive/10 px-1.5 text-[0.625rem] text-destructive">
                    {unseenFailures}
                  </span>
                ) : null
              }
            />
          )
        )}
      </nav>
    </section>
  )
}
