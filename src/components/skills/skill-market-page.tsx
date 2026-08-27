"use client"

import { Suspense, useCallback, useState } from "react"
import {
  CapabilityMarketShell,
  type CapabilityMarketSection,
} from "@/components/skills/market/capability-market-shell"
import { ConnectorMarketView } from "@/components/skills/market/connector-market-view"
import { PluginMarketPreview } from "@/components/skills/market/plugin-market-preview"
import { SkillMarketView } from "@/components/skills/market/view"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { cn } from "@/lib/utils"

export function SkillMarketPage() {
  const { skillMarketTarget, consumeSkillMarketTarget, openSkillMarket } =
    useWorkbenchRoute()
  const [section, setSection] = useState<CapabilityMarketSection>("skills")
  const [visited, setVisited] = useState<Set<CapabilityMarketSection>>(
    () => new Set(["skills"])
  )

  const openSection = useCallback((next: CapabilityMarketSection) => {
    setVisited((current) => {
      if (current.has(next)) return current
      return new Set(current).add(next)
    })
    setSection(next)
  }, [])

  return (
    <Suspense fallback={null}>
      <CapabilityMarketShell
        activeSection={section}
        onSectionChange={openSection}
      >
        <div className={cn("h-full", section !== "skills" && "hidden")}>
          <SkillMarketView
            navigationTarget={skillMarketTarget}
            onNavigationTargetConsumed={consumeSkillMarketTarget}
            onOpenConnectors={() => openSection("connectors")}
          />
        </div>
        {visited.has("connectors") ? (
          <div className={cn("h-full", section !== "connectors" && "hidden")}>
            <ConnectorMarketView />
          </div>
        ) : null}
        {visited.has("plugins") ? (
          <div className={cn("h-full", section !== "plugins" && "hidden")}>
            <PluginMarketPreview
              onOpenPlugin={(slug) => {
                openSection("skills")
                openSkillMarket(slug)
              }}
            />
          </div>
        ) : null}
      </CapabilityMarketShell>
    </Suspense>
  )
}
